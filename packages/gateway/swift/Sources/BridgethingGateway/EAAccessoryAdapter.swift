#if canImport(ExternalAccessory)

  import ExternalAccessory
  import Foundation

  /// `Adapter` implementation that talks to the bridgething daemon over an
  /// MFi-paired iAP2 session, exposed to apps as an `EAAccessory`. Filters the
  /// list of connected accessories by a declared protocol string and opens an
  /// `EASession` on each match.
  ///
  /// Lifecycle on iOS:
  /// 1. Pair the Car Thing once via Settings -> Bluetooth (one-time MFi auth).
  /// 2. Declare the protocol string under `UISupportedExternalAccessoryProtocols`
  ///    in the consuming app's Info.plist.
  /// 3. For backgrounded operation also add `external-accessory` to
  ///    `UIBackgroundModes`. App Store distribution gates `external-accessory`
  ///    background mode through review; sideloaded apps still get it.
  ///
  /// Threading: streams are scheduled on the main RunLoop, so all stream
  /// delegate callbacks run on the main thread. Public Adapter methods marshal
  /// onto the main thread via `MainActor.run`. RFCOMM bandwidth (~700 kbps peak)
  /// is comfortably below what the main RunLoop can sustain without UI hitches.
  public final class EAAccessoryAdapter: NSObject, Adapter, @unchecked Sendable {
    public nonisolated let events: AsyncStream<AdapterEvent>
    private let eventContinuation: AsyncStream<AdapterEvent>.Continuation

    private let protocolString: String

    // The trailing state is only read or mutated from the main thread: public methods hop via
    // MainActor.run and stream/notification callbacks fire on main because we scheduled them there.
    private var sessions: [String: SessionState] = [:]
    private var observers: [NSObjectProtocol] = []
    private var started = false

    public init(protocolString: String) {
      self.protocolString = protocolString
      let (stream, continuation) = AsyncStream.makeStream(of: AdapterEvent.self)
      events = stream
      eventContinuation = continuation
      super.init()
    }

    public func start() async throws {
      await MainActor.run {
        guard !self.started else { return }
        self.started = true

        let center = NotificationCenter.default
        let connectObserver = center.addObserver(
          forName: .EAAccessoryDidConnect, object: nil, queue: .main
        ) { [weak self] note in
          guard
            let self,
            let accessory = note.userInfo?[EAAccessoryKey] as? EAAccessory
          else { return }
          tryOpenSession(for: accessory)
        }
        let disconnectObserver = center.addObserver(
          forName: .EAAccessoryDidDisconnect, object: nil, queue: .main
        ) { [weak self] note in
          guard
            let self,
            let accessory = note.userInfo?[EAAccessoryKey] as? EAAccessory
          else { return }
          handleDisconnect(deviceId: Self.deviceId(for: accessory))
        }
        self.observers = [connectObserver, disconnectObserver]
        EAAccessoryManager.shared().registerForLocalNotifications()

        for accessory in EAAccessoryManager.shared().connectedAccessories {
          self.tryOpenSession(for: accessory)
        }
      }
    }

    public func stop() async {
      await MainActor.run {
        guard self.started else { return }
        let center = NotificationCenter.default
        for observer in self.observers {
          center.removeObserver(observer)
        }
        self.observers.removeAll()
        EAAccessoryManager.shared().unregisterForLocalNotifications()

        for (_, session) in self.sessions {
          session.tearDown()
        }
        self.sessions.removeAll()
        self.started = false
        self.eventContinuation.finish()
      }
    }

    public func disconnect(deviceId: String) async throws {
      let known = await MainActor.run { () -> Bool in
        guard let session = self.sessions.removeValue(forKey: deviceId) else { return false }
        session.tearDown()
        self.eventContinuation.yield(.disconnected(deviceId: deviceId))
        return true
      }
      if !known { throw AdapterError.unknownDevice(deviceId) }
    }

    public func send(deviceId: String, frame: Data) async throws {
      let result: Result<Void, AdapterError> = await MainActor.run {
        guard let session = self.sessions[deviceId] else {
          return .failure(.unknownDevice(deviceId))
        }
        session.enqueueWrite(frame)
        return .success(())
      }
      try result.get()
    }

    // MARK: - main-thread internals

    fileprivate func handleInbound(deviceId: String, bytes: Data) {
      eventContinuation.yield(.bytes(deviceId: deviceId, bytes))
    }

    fileprivate func handleStreamEnd(deviceId: String) {
      if let session = sessions.removeValue(forKey: deviceId) {
        session.tearDown()
        eventContinuation.yield(.disconnected(deviceId: deviceId))
      }
    }

    private func handleDisconnect(deviceId: String) {
      if let session = sessions.removeValue(forKey: deviceId) {
        session.tearDown()
        eventContinuation.yield(.disconnected(deviceId: deviceId))
      }
    }

    private func tryOpenSession(for accessory: EAAccessory) {
      guard accessory.protocolStrings.contains(protocolString) else { return }
      let id = Self.deviceId(for: accessory)
      if sessions[id] != nil { return }
      // Session creation fails when iOS rejects the accessory (e.g. protocol string not declared in
      // UISupportedExternalAccessoryProtocols). No "connected" event was emitted, so nothing to undo.
      guard let session = EASession(accessory: accessory, forProtocol: protocolString) else {
        return
      }
      let state = SessionState(accessory: accessory, session: session, owner: self)
      sessions[id] = state
      eventContinuation.yield(.connected(Device(id: id, name: accessory.name)))
    }

    private static func deviceId(for accessory: EAAccessory) -> String {
      let serial = accessory.serialNumber
      return serial.isEmpty ? "ea-\(accessory.connectionID)" : serial
    }
  }

  private final class SessionState: NSObject, StreamDelegate, @unchecked Sendable {
    let accessory: EAAccessory
    let session: EASession
    weak var owner: EAAccessoryAdapter?
    var pendingWrites = Data()

    var deviceId: String {
      let serial = accessory.serialNumber
      return serial.isEmpty ? "ea-\(accessory.connectionID)" : serial
    }

    init(accessory: EAAccessory, session: EASession, owner: EAAccessoryAdapter) {
      self.accessory = accessory
      self.session = session
      self.owner = owner
      super.init()
      if let input = session.inputStream {
        input.delegate = self
        input.schedule(in: .main, forMode: .default)
        input.open()
      }
      if let output = session.outputStream {
        output.delegate = self
        output.schedule(in: .main, forMode: .default)
        output.open()
      }
    }

    func tearDown() {
      if let input = session.inputStream {
        input.close()
        input.remove(from: .main, forMode: .default)
        input.delegate = nil
      }
      if let output = session.outputStream {
        output.close()
        output.remove(from: .main, forMode: .default)
        output.delegate = nil
      }
    }

    func enqueueWrite(_ data: Data) {
      pendingWrites.append(data)
      drainOutput()
    }

    private func drainOutput() {
      guard let out = session.outputStream else { return }
      while !pendingWrites.isEmpty, out.hasSpaceAvailable {
        let written = pendingWrites.withUnsafeBytes { raw -> Int in
          guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
          return out.write(base, maxLength: pendingWrites.count)
        }
        if written <= 0 { break }
        pendingWrites.removeSubrange(0 ..< written)
      }
    }

    func stream(_ aStream: Stream, handle eventCode: Stream.Event) {
      switch eventCode {
      case .hasBytesAvailable:
        guard let input = aStream as? InputStream else { return }
        var buffer = [UInt8](repeating: 0, count: 4096)
        while input.hasBytesAvailable {
          let n = buffer.withUnsafeMutableBufferPointer { ptr -> Int in
            guard let base = ptr.baseAddress else { return 0 }
            return input.read(base, maxLength: ptr.count)
          }
          if n <= 0 { break }
          owner?.handleInbound(deviceId: deviceId, bytes: Data(buffer.prefix(n)))
        }
      case .hasSpaceAvailable:
        drainOutput()
      case .endEncountered, .errorOccurred:
        owner?.handleStreamEnd(deviceId: deviceId)
      default:
        break
      }
    }
  }

#endif
