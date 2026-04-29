import Foundation
@testable import BridgethingGateway

/// Test-only `Adapter` that lets a test drive byte/connection events into the
/// gateway and pull back outbound frames. Not thread-safe beyond what
/// `AsyncStream`'s continuations already guarantee, which is sufficient for
/// single-test usage.
final class MockAdapter: Adapter, @unchecked Sendable {
  let events: AsyncStream<AdapterEvent>
  private let eventContinuation: AsyncStream<AdapterEvent>.Continuation

  let sentFrames: AsyncStream<(deviceId: String, frame: Data)>
  private let sentContinuation: AsyncStream<(deviceId: String, frame: Data)>.Continuation

  var startCalled = false
  var stopCalled = false
  var disconnectCalls: [String] = []

  init() {
    let (e, ec) = AsyncStream.makeStream(of: AdapterEvent.self)
    let (s, sc) = AsyncStream.makeStream(of: (deviceId: String, frame: Data).self)
    events = e
    eventContinuation = ec
    sentFrames = s
    sentContinuation = sc
  }

  func start() async throws { startCalled = true }
  func stop() async {
    stopCalled = true
    eventContinuation.finish()
    sentContinuation.finish()
  }

  func disconnect(deviceId: String) async throws {
    disconnectCalls.append(deviceId)
  }

  func send(deviceId: String, frame: Data) async throws {
    sentContinuation.yield((deviceId: deviceId, frame: frame))
  }

  func simulate(_ event: AdapterEvent) {
    eventContinuation.yield(event)
  }
}
