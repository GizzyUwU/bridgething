import BridgethingSchema
import Foundation

public enum GatewayEvent: Sendable {
  case connected(Device)
  case disconnected(deviceId: String)
  case message(deviceId: String, BridgeToGatewayMsg)
  case decodeError(deviceId: String, description: String)
  case linkFailed(device: Device, reason: String)
}

public enum BridgethingGatewayError: Error, Sendable {
  case notRunning
  case alreadyRunning
  case requestTimedOut
  case shutdown
  case unexpectedResponse(String)
}

/// Typed phone-side facade over an `Adapter`.
///
/// Owns one `FrameAccumulator` per connected device, decodes incoming frames
/// into `BridgeToGatewayMsg`, encodes outbound `GatewayToBridgeMsg` through the
/// shared `Codec`, and tracks in-flight requests so callers can `await` a
/// matching response by id.
public actor BridgethingGateway {
  private let adapter: any Adapter
  private let codec: Codec

  private var consumerTask: Task<Void, Never>?
  private var buffers: [String: FrameAccumulator] = [:]
  private var pendingRequests: [UUID: CheckedContinuation<BridgeToGatewayMsg, Error>] = [:]

  private let broadcaster = EventBroadcaster()

  /// Async stream of gateway events. Each access returns a fresh stream
  /// subscribed to the underlying broadcaster, so multiple consumers
  /// (per-surface dispatch tasks, OTA poll-loop meta tracker, host-app
  /// observers) each receive every event independently. AsyncStream is
  /// unicast at the iterator level: a single underlying stream would
  /// silently partition events across concurrent `for await` loops, so
  /// the broadcaster fans yields out to each subscribed continuation.
  public nonisolated var events: AsyncStream<GatewayEvent> {
    broadcaster.subscribe()
  }

  public init(adapter: any Adapter, codec: Codec = Codec()) {
    self.adapter = adapter
    self.codec = codec
  }

  public func start() async throws {
    guard consumerTask == nil else { throw BridgethingGatewayError.alreadyRunning }
    try await adapter.start()
    let stream = adapter.events
    consumerTask = Task { [weak self] in
      for await event in stream {
        await self?.handleAdapterEvent(event)
      }
    }
  }

  public func stop() async {
    consumerTask?.cancel()
    consumerTask = nil
    await adapter.stop()

    for (_, cont) in pendingRequests {
      cont.resume(throwing: BridgethingGatewayError.shutdown)
    }
    pendingRequests.removeAll()
    buffers.removeAll()
    broadcaster.finish()
  }

  public func disconnect(deviceId: String) async throws {
    try await adapter.disconnect(deviceId: deviceId)
  }

  /// Snapshot of currently connected peer ids.
  public func connectedDeviceIds() -> [String] {
    Array(buffers.keys)
  }

  /// Encode and ship a fully-formed message. Caller is responsible for picking
  /// `meta` (`.command`, `.event`, etc.). For request/response, prefer
  /// `request(deviceId:_:timeout:)` which handles id generation and awaiting.
  ///
  /// `priority` is a transport-level scheduling hint: Bulk yields to Normal at
  /// frame boundaries so latency-sensitive traffic (NowPlaying deltas, like
  /// taps) interleaves between long bulk transfers (file/OTA chunks). Default
  /// is `.normal`.
  public func send(
    deviceId: String,
    _ message: GatewayToBridgeMsg,
    priority: Priority = .normal
  ) async throws {
    let frame = try codec.encode(message, priority: priority)
    try await adapter.send(deviceId: deviceId, frame: frame)
  }

  /// Bulk-priority shorthand for `send(deviceId:_:priority:)`.
  public func sendBulk(deviceId: String, _ message: GatewayToBridgeMsg) async throws {
    try await send(deviceId: deviceId, message, priority: .bulk)
  }

  /// Send a request and await the matching response by id. The wire id is
  /// generated here and matched against `BridgeToGatewayMsg.meta.response.requestId`
  /// on the way back; non-response messages with the same id (shouldn't happen,
  /// but we don't trust the wire) flow through the event stream as usual.
  public func request(
    deviceId: String,
    _ data: GatewayToBridgeMsgData,
    timeout: Duration = .seconds(30)
  ) async throws -> BridgeToGatewayMsg {
    let id = UUID()
    let msg = GatewayToBridgeMsg(id: id, meta: .request, data: data)
    let frame = try codec.encode(msg)

    return try await withCheckedThrowingContinuation { cont in
      pendingRequests[id] = cont

      Task { [weak self] in
        do {
          try await self?.adapter.send(deviceId: deviceId, frame: frame)
        } catch {
          await self?.failPendingRequest(id: id, with: error)
        }
      }

      Task { [weak self] in
        try? await Task.sleep(for: timeout)
        await self?.failPendingRequest(id: id, with: BridgethingGatewayError.requestTimedOut)
      }
    }
  }

  // MARK: - private

  private func failPendingRequest(id: UUID, with error: Error) {
    if let cont = pendingRequests.removeValue(forKey: id) {
      cont.resume(throwing: error)
    }
  }

  private func completePendingRequest(id: UUID, with msg: BridgeToGatewayMsg) -> Bool {
    guard let cont = pendingRequests.removeValue(forKey: id) else { return false }
    cont.resume(returning: msg)
    return true
  }

  private func handleAdapterEvent(_ event: AdapterEvent) {
    switch event {
    case let .connected(device):
      buffers[device.id] = FrameAccumulator()
      broadcaster.emit(.connected(device))
    case let .disconnected(id):
      buffers.removeValue(forKey: id)
      broadcaster.emit(.disconnected(deviceId: id))
    case let .linkFailed(id, name, reason):
      buffers.removeValue(forKey: id)
      broadcaster.emit(.linkFailed(device: Device(id: id, name: name), reason: reason))
    case let .bytes(id, chunk):
      ingest(deviceId: id, chunk: chunk)
    }
  }

  public func reconnect(deviceId: String) async throws {
    try await adapter.reconnect(deviceId: deviceId)
  }

  private func ingest(deviceId: String, chunk: Data) {
    var accumulator = buffers[deviceId] ?? FrameAccumulator()
    accumulator.append(chunk)
    do {
      while let frame = try accumulator.nextFrame() {
        let msg = try codec.decode(BridgeToGatewayMsg.self, from: frame)
        if case let .response(r) = msg.meta, completePendingRequest(id: r.requestId, with: msg) {
          continue
        }
        broadcaster.emit(.message(deviceId: deviceId, msg))
      }
      buffers[deviceId] = accumulator
    } catch {
      buffers[deviceId] = FrameAccumulator()
      broadcaster.emit(.decodeError(deviceId: deviceId, description: String(describing: error)))
    }
  }
}

/// Multi-consumer fan-out for `GatewayEvent`s. Three jobs in one class:
///
/// 1. Each `subscribe()` returns a fresh `AsyncStream`; `emit()` yields
///    every event to every active continuation. AsyncStream itself is
///    unicast across iterators, so without this fan-out concurrent
///    `for await event in gateway.events` loops would silently partition
///    events between consumers.
/// 2. The startup race (`consumerTask` may emit events before the
///    companion's dispatcher tasks finish subscribing) is closed by
///    a bounded replay cache. Each new subscriber receives the buffered
///    history under the same lock that emit takes, so a concurrent
///    emit can't interleave between the replay and going live.
/// 3. The cache is capped at a small constant so a long-running
///    session that grows new subscribers doesn't leak memory; in
///    practice the bridgething companion subscribes its full dispatcher
///    set at startup and adds nothing later.
final class EventBroadcaster: @unchecked Sendable {
  private static let replayCacheLimit = 256

  private let lock = NSLock()
  private var subscribers: [UUID: AsyncStream<GatewayEvent>.Continuation] = [:]
  private var replay: [GatewayEvent] = []
  private var finished = false

  func subscribe() -> AsyncStream<GatewayEvent> {
    AsyncStream(bufferingPolicy: .bufferingNewest(1024)) { continuation in
      let id = UUID()
      lock.lock()
      if finished {
        lock.unlock()
        continuation.finish()
        return
      }
      for e in replay {
        continuation.yield(e)
      }
      subscribers[id] = continuation
      lock.unlock()
      continuation.onTermination = { [weak self] _ in
        guard let self else { return }
        lock.lock()
        subscribers.removeValue(forKey: id)
        lock.unlock()
      }
    }
  }

  func emit(_ event: GatewayEvent) {
    lock.lock()
    replay.append(event)
    if replay.count > Self.replayCacheLimit {
      replay.removeFirst(replay.count - Self.replayCacheLimit)
    }
    let copies = Array(subscribers.values)
    lock.unlock()
    for c in copies {
      c.yield(event)
    }
  }

  func finish() {
    lock.lock()
    let copies = Array(subscribers.values)
    subscribers.removeAll()
    replay.removeAll()
    finished = true
    lock.unlock()
    for c in copies {
      c.finish()
    }
  }
}
