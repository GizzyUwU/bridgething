import BridgethingSchema
import Foundation

public enum GatewayEvent: Sendable {
  case connected(Device)
  case disconnected(deviceId: String)
  case message(deviceId: String, BridgeToGatewayMsg)
  /// Surfaced when a frame fails to parse or decode. The peer is still
  /// considered connected; consumers may choose to disconnect on repeated
  /// errors. `description` carries the underlying error rendered as a string
  /// (we don't expose the live `Error` because not every error type on the
  /// codec/decoder path is `Sendable`).
  case decodeError(deviceId: String, description: String)
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
  private var pendingRequests: [Data: CheckedContinuation<BridgeToGatewayMsg, Error>] = [:]

  public nonisolated let events: AsyncStream<GatewayEvent>
  private let eventContinuation: AsyncStream<GatewayEvent>.Continuation

  public init(adapter: any Adapter, codec: Codec = Codec()) {
    self.adapter = adapter
    self.codec = codec
    let (stream, continuation) = AsyncStream.makeStream(of: GatewayEvent.self)
    events = stream
    eventContinuation = continuation
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
    eventContinuation.finish()
  }

  public func disconnect(deviceId: String) async throws {
    try await adapter.disconnect(deviceId: deviceId)
  }

  /// Encode and ship a fully-formed message. Caller is responsible for picking
  /// `meta` (`.command`, `.event`, etc.). For request/response, prefer
  /// `request(deviceId:_:timeout:)` which handles id generation and awaiting.
  ///
  /// `priority` is a transport-level scheduling hint - Bulk yields to Normal at
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
    let id = UUID().data
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

  private func failPendingRequest(id: Data, with error: Error) {
    if let cont = pendingRequests.removeValue(forKey: id) {
      cont.resume(throwing: error)
    }
  }

  private func completePendingRequest(id: Data, with msg: BridgeToGatewayMsg) -> Bool {
    guard let cont = pendingRequests.removeValue(forKey: id) else { return false }
    cont.resume(returning: msg)
    return true
  }

  private func handleAdapterEvent(_ event: AdapterEvent) {
    switch event {
    case .connected(let device):
      buffers[device.id] = FrameAccumulator()
      eventContinuation.yield(.connected(device))
    case .disconnected(let id):
      buffers.removeValue(forKey: id)
      eventContinuation.yield(.disconnected(deviceId: id))
    case .bytes(let id, let chunk):
      ingest(deviceId: id, chunk: chunk)
    }
  }

  private func ingest(deviceId: String, chunk: Data) {
    var accumulator = buffers[deviceId] ?? FrameAccumulator()
    accumulator.append(chunk)
    do {
      while let frame = try accumulator.nextFrame() {
        let msg = try codec.decode(BridgeToGatewayMsg.self, from: frame)
        if case .response(let r) = msg.meta, completePendingRequest(id: r.requestId, with: msg) {
          continue
        }
        eventContinuation.yield(.message(deviceId: deviceId, msg))
      }
      buffers[deviceId] = accumulator
    } catch {
      buffers[deviceId] = FrameAccumulator()
      eventContinuation.yield(.decodeError(deviceId: deviceId, description: String(describing: error)))
    }
  }
}
