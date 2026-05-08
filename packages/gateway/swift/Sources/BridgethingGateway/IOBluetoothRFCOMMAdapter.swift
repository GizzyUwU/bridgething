#if canImport(IOBluetooth)

  import Foundation
  import IOBluetooth

  /// `Adapter` implementation for macOS that talks to the bridgething daemon
  /// over a plain RFCOMM channel (the same SPP service Android consumes -
  /// `dead0000-854d-408e-81f0-fb6147f918fd`). The host Mac app constructs
  /// this in place of `EAAccessoryAdapter` (iOS-only) and hands it to
  /// `BridgethingCompanion(adapter:)`; everything above the `Adapter`
  /// protocol - the manifest poll loop, OTA, glue, codegen - is platform
  /// agnostic and inherits Mac support for free.
  ///
  /// Lifecycle:
  /// 1. The user pairs the Car Thing once via System Settings → Bluetooth.
  /// 2. `start()` enumerates paired devices, picks the ones whose SDP
  ///    records advertise the bridgething UUID, opens an RFCOMM channel to
  ///    each, and emits `.connected` per channel that opens.
  /// 3. `IOBluetoothRFCOMMChannelDelegate` callbacks pump inbound bytes
  ///    into the `events` stream and back-pressure outbound writes via
  ///    `rfcommChannelWriteComplete`.
  /// 4. On channel close we emit `.disconnected` and arm a retry on the
  ///    next device-connect notification so the link can come back without
  ///    a manual restart.
  ///
  /// Threading: IOBluetooth callbacks fire on whichever thread opened the
  /// channel; we open on main and marshal every public method through
  /// `MainActor.run` to keep state single-threaded - same pattern as
  /// `EAAccessoryAdapter`.
  public final class IOBluetoothRFCOMMAdapter: NSObject, Adapter, @unchecked Sendable {
    public nonisolated let events: AsyncStream<AdapterEvent>
    private let eventContinuation: AsyncStream<AdapterEvent>.Continuation

    private let serviceUUID: IOBluetoothSDPUUID

    // Main-thread state. Public methods hop here via `MainActor.run`;
    // delegate callbacks already arrive on main because we open the
    // channels from main.
    private var sessions: [String: SessionState] = [:]
    private var connectNotification: IOBluetoothUserNotification?
    private var started = false

    /// Construct an adapter that targets RFCOMM services advertising the
    /// supplied 128-bit UUID. Pass `IOBluetoothRFCOMMAdapter.bridgethingUUID`
    /// for the standard companion; override for custom forks or tests.
    public init(serviceUUID: IOBluetoothSDPUUID = IOBluetoothRFCOMMAdapter.bridgethingUUID) {
      self.serviceUUID = serviceUUID
      let (stream, continuation) = AsyncStream.makeStream(of: AdapterEvent.self)
      events = stream
      eventContinuation = continuation
      super.init()
    }

    /// Standard bridgething RFCOMM service UUID, mirrors the constant
    /// `BRIDGETHING_PROFILE_UUID` in `crates/lib`.
    nonisolated(unsafe) public static let bridgethingUUID: IOBluetoothSDPUUID = {
      var bytes: [UInt8] = [
        0xDE, 0xAD, 0x00, 0x00, 0x85, 0x4D, 0x40, 0x8E,
        0x81, 0xF0, 0xFB, 0x61, 0x47, 0xF9, 0x18, 0xFD,
      ]
      return bytes.withUnsafeBufferPointer { buf in
        IOBluetoothSDPUUID(bytes: buf.baseAddress, length: buf.count)
      }
    }()

    public func start() async throws {
      await MainActor.run {
        guard !self.started else { return }
        self.started = true

        // Fire when any bluetooth device connects to the host. We don't
        // care about non-bridgething devices; the SDP filter inside
        // `tryOpenChannel(for:)` handles that.
        self.connectNotification = IOBluetoothDevice.register(
          forConnectNotifications: self,
          selector: #selector(handleDeviceConnected(_:device:))
        )

        // Catch up on devices that were already connected when we
        // started; otherwise we'd wait until the next reconnect.
        for any in IOBluetoothDevice.pairedDevices() ?? [] {
          guard let device = any as? IOBluetoothDevice else { continue }
          self.tryOpenChannel(for: device)
        }
      }
    }

    public func stop() async {
      await MainActor.run {
        guard self.started else { return }
        self.connectNotification?.unregister()
        self.connectNotification = nil
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

    /// Called by `IOBluetoothDevice` when any device connects. Selector
    /// signature is fixed by IOBluetooth; the `device` argument is the
    /// peer that just came up.
    @objc fileprivate func handleDeviceConnected(_: IOBluetoothUserNotification, device: IOBluetoothDevice) {
      tryOpenChannel(for: device)
    }

    /// Called by `IOBluetoothDevice` when an SDP query we kicked off
    /// finishes. Signature is fixed by IOBluetooth; we re-enter
    /// `tryOpenChannel` so the now-populated record cache gets walked.
    @objc fileprivate func sdpQueryComplete(_ device: IOBluetoothDevice, status: IOReturn) {
      guard status == kIOReturnSuccess else { return }
      tryOpenChannel(for: device)
    }

    fileprivate func handleInbound(deviceId: String, bytes: Data) {
      eventContinuation.yield(.bytes(deviceId: deviceId, bytes))
    }

    fileprivate func handleChannelClosed(deviceId: String, hadConnected: Bool) {
      if let session = sessions.removeValue(forKey: deviceId) {
        session.tearDown()
        if hadConnected {
          eventContinuation.yield(.disconnected(deviceId: deviceId))
        }
      }
    }

    private func tryOpenChannel(for device: IOBluetoothDevice) {
      let id = Self.deviceId(for: device)
      if sessions[id] != nil { return }

      // Resolve the RFCOMM channel ID by walking the device's SDP
      // records for one matching our service UUID. Paired devices
      // typically have records cached; if not, kick off an SDP query
      // and rely on `sdpQueryComplete:status:` to re-enter here once
      // the cache populates.
      let records = device.services as? [IOBluetoothSDPServiceRecord] ?? []
      var matchedChannel: BluetoothRFCOMMChannelID?
      for record in records {
        guard record.hasService(from: [serviceUUID]) else { continue }
        var channelID: BluetoothRFCOMMChannelID = 0
        if record.getRFCOMMChannelID(&channelID) == kIOReturnSuccess {
          matchedChannel = channelID
          break
        }
      }
      guard let channelID = matchedChannel else {
        if records.isEmpty {
          device.performSDPQuery(self)
        }
        return
      }

      var channel: IOBluetoothRFCOMMChannel?
      let result = device.openRFCOMMChannelAsync(&channel, withChannelID: channelID, delegate: nil)
      guard result == kIOReturnSuccess, let channel else { return }

      let state = SessionState(device: device, channel: channel, owner: self)
      sessions[id] = state
      // `.connected` fires after the channel actually opens
      // (rfcommChannelOpenComplete), not here - opening is async and
      // can still fail on the link-layer.
    }

    fileprivate func handleChannelOpened(deviceId: String, name: String) {
      eventContinuation.yield(.connected(Device(id: deviceId, name: name)))
    }

    fileprivate static func deviceId(for device: IOBluetoothDevice) -> String {
      device.addressString ?? "iobt-\(ObjectIdentifier(device).hashValue)"
    }
  }

  /// Per-channel state + delegate. NSObject because IOBluetooth's
  /// delegate protocol is informal `@objc` and dispatches via selector.
  private final class SessionState: NSObject, IOBluetoothRFCOMMChannelDelegate, @unchecked Sendable {
    let device: IOBluetoothDevice
    let channel: IOBluetoothRFCOMMChannel
    weak var owner: IOBluetoothRFCOMMAdapter?

    private var pendingWrites = Data()
    private var inFlight = false
    private var didNotifyConnected = false

    var deviceId: String { IOBluetoothRFCOMMAdapter.deviceId(for: device) }

    init(device: IOBluetoothDevice, channel: IOBluetoothRFCOMMChannel, owner: IOBluetoothRFCOMMAdapter) {
      self.device = device
      self.channel = channel
      self.owner = owner
      super.init()
      channel.setDelegate(self)
    }

    func tearDown() {
      channel.setDelegate(nil)
      _ = channel.close()
      pendingWrites.removeAll(keepingCapacity: false)
      inFlight = false
    }

    func enqueueWrite(_ data: Data) {
      pendingWrites.append(data)
      drainOutput()
    }

    private func drainOutput() {
      guard !inFlight, !pendingWrites.isEmpty else { return }
      // RFCOMM has a per-channel MTU that caps a single writeAsync;
      // chunk the queued bytes to it. Typical MTU is ~500 bytes.
      let mtu = Int(channel.getMTU())
      let take = min(mtu, pendingWrites.count)
      let chunk = pendingWrites.prefix(take)
      pendingWrites.removeFirst(take)
      inFlight = true
      let status = chunk.withUnsafeBytes { raw -> IOReturn in
        guard let base = raw.baseAddress else { return kIOReturnNoMemory }
        return channel.writeAsync(
          UnsafeMutableRawPointer(mutating: base),
          length: UInt16(chunk.count),
          refcon: nil
        )
      }
      if status != kIOReturnSuccess {
        inFlight = false
        owner?.handleChannelClosed(deviceId: deviceId, hadConnected: didNotifyConnected)
      }
    }

    // MARK: - IOBluetoothRFCOMMChannelDelegate

    func rfcommChannelOpenComplete(_: IOBluetoothRFCOMMChannel!, status error: IOReturn) {
      if error != kIOReturnSuccess {
        owner?.handleChannelClosed(deviceId: deviceId, hadConnected: didNotifyConnected)
        return
      }
      didNotifyConnected = true
      let name = device.name ?? deviceId
      owner?.handleChannelOpened(deviceId: deviceId, name: name)
    }

    func rfcommChannelData(_: IOBluetoothRFCOMMChannel!, data dataPointer: UnsafeMutableRawPointer!, length dataLength: Int) {
      let bytes = Data(bytes: dataPointer, count: dataLength)
      owner?.handleInbound(deviceId: deviceId, bytes: bytes)
    }

    func rfcommChannelWriteComplete(_: IOBluetoothRFCOMMChannel!, refcon _: UnsafeMutableRawPointer!, status error: IOReturn) {
      inFlight = false
      if error != kIOReturnSuccess {
        owner?.handleChannelClosed(deviceId: deviceId, hadConnected: didNotifyConnected)
        return
      }
      drainOutput()
    }

    func rfcommChannelClosed(_: IOBluetoothRFCOMMChannel!) {
      owner?.handleChannelClosed(deviceId: deviceId, hadConnected: didNotifyConnected)
    }
  }

#endif
