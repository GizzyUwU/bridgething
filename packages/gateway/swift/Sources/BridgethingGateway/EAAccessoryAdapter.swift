#if canImport(ExternalAccessory)

  import BridgethingSchema
  import ExternalAccessory
  import Foundation
  import Logging

  private let eaLog = Logger(label: "ea")

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
  /// iOS reports `.EAAccessoryDidConnect` (and lists the accessory in
  /// `connectedAccessories` at cold launch) before the MFi session is actually
  /// ready, so `EASession(accessory:forProtocol:)` returns nil for a few seconds.
  /// Opens therefore retry with backoff, and the peer is reported connected only
  /// once both streams reach `.openCompleted` (not when the session object is
  /// created). Exhausting the fast retries yields `.linkFailed` once, then a slow
  /// background retry keeps running so the link self-heals when the accessory is
  /// ready again (recreating an `EASession` races the previous one's async release,
  /// so a single fast burst is not enough on a mid-session drop).
  /// Dedicated thread whose run loop carries all EA stream I/O. Keeping the
  /// streams off the main run loop decouples link throughput and latency from
  /// UI work on the main thread.
  private final class EAIOThread {
    private let thread: Thread
    private let cfLoop: CFRunLoop

    init() {
      let ready = DispatchSemaphore(value: 0)
      let box = UnsafeMutablePointer<CFRunLoop?>.allocate(capacity: 1)
      box.initialize(to: nil)
      thread = Thread {
        box.pointee = CFRunLoopGetCurrent()
        ready.signal()
        RunLoop.current.add(NSMachPort(), forMode: .default)
        while !Thread.current.isCancelled {
          RunLoop.current.run(mode: .default, before: .distantFuture)
        }
      }
      thread.name = "bridgething-ea-io"
      thread.qualityOfService = .userInitiated
      thread.start()
      ready.wait()
      cfLoop = box.pointee!
      box.deinitialize(count: 1)
      box.deallocate()
    }

    func perform(_ block: @escaping () -> Void) {
      CFRunLoopPerformBlock(cfLoop, CFRunLoopMode.defaultMode.rawValue, block)
      CFRunLoopWakeUp(cfLoop)
    }

    func stop() {
      thread.cancel()
      CFRunLoopWakeUp(cfLoop)
    }
  }

  public final class EAAccessoryAdapter: NSObject, Adapter, @unchecked Sendable {
    public nonisolated let events: AsyncStream<AdapterEvent>
    private let eventContinuation: AsyncStream<AdapterEvent>.Continuation

    private let protocolString: String
    private let maxOpenAttempts = 6
    private let slowRetryInterval = 5.0
    private let ioThread = EAIOThread()

    private var sessions: [String: SessionState] = [:]
    private var linkedUp: Set<String> = []
    private var retryTasks: [String: Task<Void, Never>] = [:]
    private var linkFailedReported: Set<String> = []
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

        for (_, task) in self.retryTasks { task.cancel() }
        self.retryTasks.removeAll()
        self.linkFailedReported.removeAll()
        for (_, session) in self.sessions {
          self.ioThread.perform { session.tearDown() }
        }
        self.sessions.removeAll()
        self.linkedUp.removeAll()
        self.ioThread.stop()
        self.started = false
        self.eventContinuation.finish()
      }
    }

    public func disconnect(deviceId: String) async throws {
      let known = await MainActor.run { () -> Bool in
        self.retryTasks.removeValue(forKey: deviceId)?.cancel()
        self.linkFailedReported.remove(deviceId)
        self.linkedUp.remove(deviceId)
        guard let session = self.sessions.removeValue(forKey: deviceId) else { return false }
        self.ioThread.perform { session.tearDown() }
        self.eventContinuation.yield(.disconnected(deviceId: deviceId))
        return true
      }
      if !known { throw AdapterError.unknownDevice(deviceId) }
    }

    public func reconnect(deviceId: String) async throws {
      try await MainActor.run {
        self.retryTasks.removeValue(forKey: deviceId)?.cancel()
        self.linkFailedReported.remove(deviceId)
        self.linkedUp.remove(deviceId)
        if let session = self.sessions.removeValue(forKey: deviceId) {
          self.ioThread.perform { session.tearDown() }
        }
        guard let accessory = EAAccessoryManager.shared().connectedAccessories
          .first(where: { Self.deviceId(for: $0) == deviceId })
        else { throw AdapterError.unknownDevice(deviceId) }
        self.tryOpenSession(for: accessory)
      }
    }

    public func send(deviceId: String, frame: Data) async throws {
      let session: SessionState? = await MainActor.run {
        guard self.linkedUp.contains(deviceId) else { return nil }
        return self.sessions[deviceId]
      }
      guard let session else { throw AdapterError.unknownDevice(deviceId) }
      ioThread.perform { session.enqueueWrite(frame) }
    }

    // MARK: - main-thread internals

    fileprivate func handleInbound(deviceId: String, bytes: Data) {
      eventContinuation.yield(.bytes(deviceId: deviceId, bytes))
    }

    /// Both streams reached `.openCompleted`; the link is usable.
    fileprivate func linkUp(_ state: SessionState) {
      let id = state.deviceId
      guard sessions[id] === state else { return }
      linkedUp.insert(id)
      retryTasks.removeValue(forKey: id)?.cancel()
      linkFailedReported.remove(id)
      eaLog.info("ea link up for \(id)")
      eventContinuation.yield(.connected(Device(id: id, name: state.accessory.name)))
    }

    /// A stream errored or closed before the link came up; tear down and retry.
    fileprivate func linkOpenFailed(_ state: SessionState, reason: String) {
      let id = state.deviceId
      eaLog.warning("ea open failed for \(id) (attempt \(state.attempt + 1)): \(reason)")
      if sessions[id] === state { sessions.removeValue(forKey: id) }
      ioThread.perform { state.tearDown() }
      scheduleRetryOrFail(accessory: state.accessory, attempt: state.attempt, reason: reason)
    }

    fileprivate func linkDropped(_ state: SessionState, reason: String) {
      let id = state.deviceId
      guard sessions[id] === state else { return }
      sessions.removeValue(forKey: id)
      linkedUp.remove(id)
      ioThread.perform { state.tearDown() }
      let stillAttached = EAAccessoryManager.shared().connectedAccessories
        .contains { Self.deviceId(for: $0) == id }
      if stillAttached {
        eaLog.warning("ea link dropped for \(id) after link-up (\(reason)); re-opening")
        scheduleRetryOrFail(accessory: state.accessory, attempt: 0, reason: reason)
      } else {
        eaLog.info("ea link ended for \(id) (\(reason)); accessory gone")
        linkFailedReported.remove(id)
        eventContinuation.yield(.disconnected(deviceId: id))
      }
    }

    private func handleDisconnect(deviceId: String) {
      retryTasks.removeValue(forKey: deviceId)?.cancel()
      linkFailedReported.remove(deviceId)
      linkedUp.remove(deviceId)
      if let session = sessions.removeValue(forKey: deviceId) {
        ioThread.perform { session.tearDown() }
        eventContinuation.yield(.disconnected(deviceId: deviceId))
      }
    }

    private func tryOpenSession(for accessory: EAAccessory, attempt: Int = 0) {
      guard accessory.protocolStrings.contains(protocolString) else { return }
      let id = Self.deviceId(for: accessory)
      if sessions[id] != nil { return }

      guard let session = EASession(accessory: accessory, forProtocol: protocolString) else {
        scheduleRetryOrFail(
          accessory: accessory, attempt: attempt,
          reason: "EASession(accessory:forProtocol:) returned nil"
        )
        return
      }
      let state = SessionState(accessory: accessory, session: session, owner: self, attempt: attempt)
      sessions[id] = state
      ioThread.perform { state.openStreams() }
    }

    private func scheduleRetryOrFail(accessory: EAAccessory, attempt: Int, reason: String) {
      let id = Self.deviceId(for: accessory)
      let exhausted = attempt + 1 >= maxOpenAttempts
      if exhausted, linkFailedReported.insert(id).inserted {
        eaLog.error("link failed for \(id) after \(attempt + 1) attempts: \(reason); continuing slow retry")
        eventContinuation.yield(.linkFailed(deviceId: id, name: accessory.name, reason: reason))
      }
      let delay = exhausted ? slowRetryInterval : min(0.5 * pow(2.0, Double(attempt)), 4.0)
      let nextAttempt = exhausted ? attempt : attempt + 1
      retryTasks[id]?.cancel()
      retryTasks[id] = Task { @MainActor [weak self] in
        try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
        guard let self, !Task.isCancelled else { return }
        self.retryTasks.removeValue(forKey: id)
        guard let next = EAAccessoryManager.shared().connectedAccessories
          .first(where: { Self.deviceId(for: $0) == id })
        else {
          self.linkFailedReported.remove(id)
          return
        }
        self.tryOpenSession(for: next, attempt: nextAttempt)
      }
    }

    private static func deviceId(for accessory: EAAccessory) -> String {
      let serial = accessory.serialNumber
      return serial.isEmpty ? "ea-\(accessory.connectionID)" : serial
    }
  }

  private final class SessionState: NSObject, StreamDelegate, @unchecked Sendable {
    let accessory: EAAccessory
    let session: EASession
    let attempt: Int
    weak var owner: EAAccessoryAdapter?
    private var normalQueue: [Data] = []
    private var bulkQueue: [Data] = []
    private var backgroundQueue: [Data] = []
    private var currentWrite = Data()
    private var queuedBytes = 0
    private let highWaterBytes = 4 << 20
    private let hardCapBytes = 8 << 20

    private var inputOpen = false
    private var outputOpen = false
    private var isLinkedUp = false
    private var openFailed = false

    var deviceId: String {
      let serial = accessory.serialNumber
      return serial.isEmpty ? "ea-\(accessory.connectionID)" : serial
    }

    init(accessory: EAAccessory, session: EASession, owner: EAAccessoryAdapter, attempt: Int) {
      self.accessory = accessory
      self.session = session
      self.owner = owner
      self.attempt = attempt
      super.init()
    }

    func openStreams() {
      if let input = session.inputStream {
        input.delegate = self
        input.schedule(in: .current, forMode: .default)
        input.open()
      }
      if let output = session.outputStream {
        output.delegate = self
        output.schedule(in: .current, forMode: .default)
        output.open()
      }
    }

    func tearDown() {
      if let input = session.inputStream {
        input.close()
        input.remove(from: .current, forMode: .default)
        input.delegate = nil
      }
      if let output = session.outputStream {
        output.close()
        output.remove(from: .current, forMode: .default)
        output.delegate = nil
      }
    }

    func enqueueWrite(_ frame: Data) {
      switch frame.count >= 16 ? Priority.fromByte(frame[frame.startIndex + 5]) : .normal {
      case .normal: normalQueue.append(frame)
      case .bulk: bulkQueue.append(frame)
      case .background: backgroundQueue.append(frame)
      }
      queuedBytes += frame.count
      enforceBackpressure()
      drainOutput()
    }

    private func enforceBackpressure() {
      while queuedBytes > highWaterBytes, !backgroundQueue.isEmpty || !bulkQueue.isEmpty {
        let dropped = backgroundQueue.isEmpty ? bulkQueue.removeFirst() : backgroundQueue.removeFirst()
        queuedBytes -= dropped.count
      }
      if queuedBytes > hardCapBytes {
        eaLog.warning("ea writer backlog \(queuedBytes) bytes over hard cap for \(deviceId); dropping stalled link")
        dropLink(reason: "writer backlog exceeded")
      }
    }

    private func dropLink(reason: String) {
      let owner = owner
      Task { @MainActor in owner?.linkDropped(self, reason: reason) }
    }

    private func drainOutput() {
      guard let out = session.outputStream else { return }
      while out.hasSpaceAvailable {
        if currentWrite.isEmpty {
          if !normalQueue.isEmpty {
            currentWrite = normalQueue.removeFirst()
          } else if !bulkQueue.isEmpty {
            currentWrite = bulkQueue.removeFirst()
          } else if !backgroundQueue.isEmpty {
            currentWrite = backgroundQueue.removeFirst()
          } else {
            break
          }
        }
        let written = currentWrite.withUnsafeBytes { raw -> Int in
          guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
          return out.write(base, maxLength: currentWrite.count)
        }
        if written < 0 {
          eaLog.warning("ea write error for \(deviceId): \(String(describing: out.streamError)); dropping link")
          dropLink(reason: "write error")
          return
        }
        if written <= 0 { break }
        currentWrite.removeSubrange(0 ..< written)
        queuedBytes -= written
      }
    }

    func stream(_ aStream: Stream, handle eventCode: Stream.Event) {
      switch eventCode {
      case .openCompleted:
        if aStream === session.inputStream { inputOpen = true }
        if aStream === session.outputStream { outputOpen = true }
        if inputOpen, outputOpen, !isLinkedUp {
          isLinkedUp = true
          let owner = owner
          Task { @MainActor in owner?.linkUp(self) }
        }
      case .hasBytesAvailable:
        guard let input = aStream as? InputStream else { return }
        var buffer = [UInt8](repeating: 0, count: 4096)
        while input.hasBytesAvailable {
          let n = buffer.withUnsafeMutableBufferPointer { ptr -> Int in
            guard let base = ptr.baseAddress else { return 0 }
            return input.read(base, maxLength: ptr.count)
          }
          if n < 0 {
            eaLog.warning("ea read error for \(deviceId): \(String(describing: input.streamError)); dropping link")
            dropLink(reason: "read error")
            return
          }
          if n <= 0 { break }
          owner?.handleInbound(deviceId: deviceId, bytes: Data(buffer.prefix(n)))
        }
      case .hasSpaceAvailable:
        drainOutput()
      case .endEncountered, .errorOccurred:
        if isLinkedUp {
          let reason = eventCode == .errorOccurred ? "stream error after link-up" : "stream closed after link-up"
          dropLink(reason: reason)
        } else if !openFailed {
          openFailed = true
          let reason = eventCode == .errorOccurred ? "stream error during open" : "stream closed during open"
          let owner = owner
          Task { @MainActor in owner?.linkOpenFailed(self, reason: reason) }
        }
      default:
        break
      }
    }
  }

#endif
