import ExternalAccessory
import Foundation
import NitroModules

/**
 * iOS-side bridgething transport. Owns one `EASession` per active accessory
 * matching the bridgething EA protocol string and pumps raw bytes back to
 * JS via the Nitro callbacks set by the TS adapter.
 *
 * Lifecycle:
 *  1. `start()` registers EA notifications + auto-opens sessions for any
 *     accessories already connected.
 *  2. `EAAccessoryDidConnect` opens an `EASession`, schedules its streams
 *     on the main RunLoop, and emits `connected` to JS.
 *  3. Inbound bytes pump through `StreamDelegate.hasBytesAvailable` reads
 *     and surface as `onBytes(deviceId, ArrayBuffer)`.
 *  4. `send(deviceId, ArrayBuffer)` queues bytes on the session's outbound
 *     buffer; the `StreamDelegate.hasSpaceAvailable` drain loop empties it.
 *  5. `stop()` closes every active session and unregisters notifications.
 *
 * Threading: all session work hops to the main RunLoop because that's where
 * `EASession.inputStream`/`outputStream` deliver `StreamDelegate` callbacks.
 * Public Nitro methods are invoked from JS-thread; we marshal to main as
 * needed.
 */
public final class HybridBridgethingTransport: HybridBridgethingTransportSpec, @unchecked Sendable {
  /// The EA protocol string declared in the host app's Info.plist
  /// (`UISupportedExternalAccessoryProtocols`). Final naming TBD; the
  /// daemon side will declare the same string.
  public static var protocolString: String = "com.bridgething.gateway"

  private var sessions: [String: Session] = [:]
  private var observers: [NSObjectProtocol] = []
  private var started = false

  private var onConnected: ((BridgethingTransportDevice) -> Void)?
  private var onDisconnected: ((String) -> Void)?
  private var onBytes: ((String, ArrayBuffer) -> Void)?
  private var onError: ((String, String) -> Void)?

  public override init() { super.init() }

  // MARK: - Hybrid spec

  public func start() throws -> Promise<Void> {
    return Promise.async { [self] in
      await MainActor.run { self.startOnMain() }
    }
  }

  public func stop() throws -> Promise<Void> {
    return Promise.async { [self] in
      await MainActor.run { self.stopOnMain() }
    }
  }

  public func connect(deviceId: String) throws -> Promise<BridgethingTransportDevice> {
    // EA discovery is push-only on iOS; the consumer can't initiate connection.
    // If the accessory is already connected, hand back its device record;
    // otherwise wait briefly for iOS to surface it.
    return Promise.async { [self] in
      if let existing = await MainActor.run(body: { self.sessions[deviceId]?.device }) {
        return existing
      }
      // iOS won't fire a notification for an accessory we missed; check the
      // current connectedAccessories list and open synchronously if found.
      if let device = await MainActor.run(body: { self.openIfConnected(deviceId: deviceId) }) {
        return device
      }
      throw RuntimeError.error(
        withMessage:
          "device \(deviceId) is not connected. iOS surfaces EAAccessory connections via system notifications; pair via Settings and the connection will be observed automatically."
      )
    }
  }

  public func disconnect(deviceId: String) throws -> Promise<Void> {
    return Promise.async { [self] in
      await MainActor.run { self.closeSession(deviceId: deviceId, notify: true) }
    }
  }

  public func send(deviceId: String, frame: ArrayBuffer) throws -> Promise<Void> {
    // ArrayBuffer is non-owning across the boundary; copy into Data so the
    // outbound write can run after this Promise returns.
    let copy = Data(buffer: frame)
    return Promise.async { [self] in
      try await MainActor.run {
        guard let session = self.sessions[deviceId] else {
          throw RuntimeError.error(withMessage: "unknown device \(deviceId)")
        }
        session.enqueue(copy)
      }
    }
  }

  public func getKnownDevices() throws -> [BridgethingTransportDevice] {
    EAAccessoryManager.shared().connectedAccessories
      .filter { $0.protocolStrings.contains(Self.protocolString) }
      .map { Self.makeDevice(from: $0) }
  }

  public func setOnConnected(callback: @escaping (BridgethingTransportDevice) -> Void) throws {
    onConnected = callback
  }

  public func setOnDisconnected(callback: @escaping (String) -> Void) throws {
    onDisconnected = callback
  }

  public func setOnBytes(callback: @escaping (String, ArrayBuffer) -> Void) throws {
    onBytes = callback
  }

  public func setOnError(callback: @escaping (String, String) -> Void) throws {
    onError = callback
  }

  // MARK: - main-actor implementation

  @MainActor
  private func startOnMain() {
    guard !started else { return }
    started = true

    let manager = EAAccessoryManager.shared()
    manager.registerForLocalNotifications()

    let center = NotificationCenter.default
    let connectObserver = center.addObserver(
      forName: .EAAccessoryDidConnect,
      object: nil,
      queue: .main
    ) { [weak self] note in
      guard let self,
            let accessory = note.userInfo?[EAAccessoryKey] as? EAAccessory else { return }
      self.handleAccessoryConnected(accessory)
    }
    let disconnectObserver = center.addObserver(
      forName: .EAAccessoryDidDisconnect,
      object: nil,
      queue: .main
    ) { [weak self] note in
      guard let self,
            let accessory = note.userInfo?[EAAccessoryKey] as? EAAccessory else { return }
      self.handleAccessoryDisconnected(accessory)
    }
    observers = [connectObserver, disconnectObserver]

    // Pick up any accessories that are already connected.
    for accessory in manager.connectedAccessories
      where accessory.protocolStrings.contains(Self.protocolString) {
      handleAccessoryConnected(accessory)
    }
  }

  @MainActor
  private func stopOnMain() {
    guard started else { return }
    started = false

    let center = NotificationCenter.default
    for observer in observers {
      center.removeObserver(observer)
    }
    observers.removeAll()

    EAAccessoryManager.shared().unregisterForLocalNotifications()

    for (id, session) in sessions {
      session.close()
      onDisconnected?(id)
    }
    sessions.removeAll()
  }

  @MainActor
  private func handleAccessoryConnected(_ accessory: EAAccessory) {
    guard accessory.protocolStrings.contains(Self.protocolString) else { return }
    guard let session = EASession(accessory: accessory, forProtocol: Self.protocolString) else {
      onError?(
        Self.deviceId(for: accessory),
        "EASession init returned nil (accessory may be busy or out of range)"
      )
      return
    }
    let device = Self.makeDevice(from: accessory)
    let active = Session(
      device: device,
      session: session,
      onBytes: { [weak self] data in self?.onBytes?(device.id, Self.makeArrayBuffer(from: data)) },
      onClose: { [weak self] in self?.handleSessionClosed(deviceId: device.id) },
      onError: { [weak self] description in self?.onError?(device.id, description) }
    )
    sessions[device.id] = active
    active.open()
    onConnected?(device)
  }

  @MainActor
  private func handleAccessoryDisconnected(_ accessory: EAAccessory) {
    let id = Self.deviceId(for: accessory)
    closeSession(deviceId: id, notify: true)
  }

  @MainActor
  private func handleSessionClosed(deviceId: String) {
    closeSession(deviceId: deviceId, notify: true)
  }

  @MainActor
  private func closeSession(deviceId: String, notify: Bool) {
    guard let session = sessions.removeValue(forKey: deviceId) else { return }
    session.close()
    if notify { onDisconnected?(deviceId) }
  }

  @MainActor
  private func openIfConnected(deviceId: String) -> BridgethingTransportDevice? {
    let accessory = EAAccessoryManager.shared().connectedAccessories.first { acc in
      Self.deviceId(for: acc) == deviceId && acc.protocolStrings.contains(Self.protocolString)
    }
    guard let accessory else { return nil }
    handleAccessoryConnected(accessory)
    return sessions[deviceId]?.device
  }

  // MARK: - helpers

  private static func deviceId(for accessory: EAAccessory) -> String {
    if !accessory.serialNumber.isEmpty { return accessory.serialNumber }
    return "ea-\(accessory.connectionID)"
  }

  private static func makeDevice(from accessory: EAAccessory) -> BridgethingTransportDevice {
    BridgethingTransportDevice(id: deviceId(for: accessory), name: accessory.name)
  }

  private static func makeArrayBuffer(from data: Data) -> ArrayBuffer {
    ArrayBuffer.copy(data: data)
  }
}

private extension Data {
  init(buffer: ArrayBuffer) {
    self = buffer.toData(copyIfNeeded: true)
  }
}

/**
 * Per-accessory streaming state. Owns the EASession's input/output streams,
 * a write queue, and a tiny StreamDelegate adapter that pumps reads/writes
 * on the main RunLoop.
 */
@MainActor
private final class Session {
  let device: BridgethingTransportDevice
  let session: EASession
  let delegate: StreamDelegateAdapter

  private(set) var pendingWrites: Data = .init()
  private var canWrite: Bool = false
  private var closed: Bool = false

  init(
    device: BridgethingTransportDevice,
    session: EASession,
    onBytes: @escaping (Data) -> Void,
    onClose: @escaping () -> Void,
    onError: @escaping (String) -> Void
  ) {
    self.device = device
    self.session = session
    self.delegate = StreamDelegateAdapter(onBytes: onBytes, onClose: onClose, onError: onError)
  }

  func open() {
    guard let input = session.inputStream, let output = session.outputStream else {
      delegate.onError("EASession returned nil streams")
      delegate.onClose()
      return
    }
    delegate.session = self
    input.delegate = delegate
    output.delegate = delegate
    input.schedule(in: .main, forMode: .default)
    output.schedule(in: .main, forMode: .default)
    input.open()
    output.open()
  }

  func enqueue(_ chunk: Data) {
    pendingWrites.append(chunk)
    drain()
  }

  func notifyHasSpace() {
    canWrite = true
    drain()
  }

  func drain() {
    guard canWrite, let output = session.outputStream, !pendingWrites.isEmpty else { return }
    let written: Int = pendingWrites.withUnsafeBytes { raw in
      guard let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return 0 }
      return output.write(base, maxLength: pendingWrites.count)
    }
    if written < 0 {
      delegate.onError(output.streamError?.localizedDescription ?? "write failed")
      close()
      return
    }
    pendingWrites.removeFirst(written)
    if pendingWrites.isEmpty { canWrite = false }
  }

  func close() {
    guard !closed else { return }
    closed = true
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
    delegate.session = nil
  }
}

@MainActor
private final class StreamDelegateAdapter: NSObject, StreamDelegate {
  weak var session: Session?
  let onBytes: (Data) -> Void
  let onClose: () -> Void
  let onError: (String) -> Void

  private var readBuffer = [UInt8](repeating: 0, count: 4096)

  init(onBytes: @escaping (Data) -> Void, onClose: @escaping () -> Void, onError: @escaping (String) -> Void) {
    self.onBytes = onBytes
    self.onClose = onClose
    self.onError = onError
  }

  nonisolated func stream(_ aStream: Stream, handle eventCode: Stream.Event) {
    DispatchQueue.main.async { [self] in
      MainActor.assumeIsolated { handle(stream: aStream, event: eventCode) }
    }
  }

  @MainActor
  private func handle(stream: Stream, event: Stream.Event) {
    switch event {
    case .hasBytesAvailable:
      guard let input = stream as? InputStream else { return }
      let n = readBuffer.withUnsafeMutableBufferPointer { buf -> Int in
        guard let base = buf.baseAddress else { return 0 }
        return input.read(base, maxLength: buf.count)
      }
      if n > 0 {
        let chunk = Data(bytes: readBuffer, count: n)
        onBytes(chunk)
      } else if n < 0 {
        onError(input.streamError?.localizedDescription ?? "read failed")
        onClose()
      }
    case .hasSpaceAvailable:
      session?.notifyHasSpace()
    case .errorOccurred:
      onError(stream.streamError?.localizedDescription ?? "stream error")
      onClose()
    case .endEncountered:
      onClose()
    default:
      break
    }
  }
}
