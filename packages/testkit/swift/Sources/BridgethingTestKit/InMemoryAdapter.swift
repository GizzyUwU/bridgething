import BridgethingGateway
import Foundation
import os

public final class InMemoryAdapter: Adapter, @unchecked Sendable {
    public let events: AsyncStream<AdapterEvent>
    private let eventContinuation: AsyncStream<AdapterEvent>.Continuation

    public let sentFrames: AsyncStream<(deviceId: String, frame: Data)>
    private let sentContinuation: AsyncStream<(deviceId: String, frame: Data)>.Continuation

    private struct State: Sendable {
        var startCalled = false
        var stopCalled = false
        var disconnectCalls: [String] = []
        var reconnectCalls: [String] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    public var startCalled: Bool { state.withLock { $0.startCalled } }
    public var stopCalled: Bool { state.withLock { $0.stopCalled } }
    public var disconnectCalls: [String] { state.withLock { $0.disconnectCalls } }
    public var reconnectCalls: [String] { state.withLock { $0.reconnectCalls } }

    public init() {
        let (e, ec) = AsyncStream.makeStream(of: AdapterEvent.self)
        let (s, sc) = AsyncStream.makeStream(of: (deviceId: String, frame: Data).self)
        events = e
        eventContinuation = ec
        sentFrames = s
        sentContinuation = sc
    }

    public func start() async throws {
        state.withLock { $0.startCalled = true }
    }

    public func stop() async {
        state.withLock { $0.stopCalled = true }
        eventContinuation.finish()
        sentContinuation.finish()
    }

    public func disconnect(deviceId: String) async throws {
        state.withLock { $0.disconnectCalls.append(deviceId) }
    }

    public func reconnect(deviceId: String) async throws {
        state.withLock { $0.reconnectCalls.append(deviceId) }
    }

    public func send(deviceId: String, frame: Data) async throws {
        sentContinuation.yield((deviceId: deviceId, frame: frame))
    }

    // MARK: - test drivers

    /// Feed an arbitrary adapter event (connect / disconnect / bytes).
    public func simulate(_ event: AdapterEvent) {
        eventContinuation.yield(event)
    }

    /// Simulate a peer (the daemon) connecting.
    public func connect(_ device: Device) {
        simulate(.connected(device))
    }

    /// Simulate inbound frame bytes from a connected peer.
    public func feed(deviceId: String, _ frame: Data) {
        simulate(.bytes(deviceId: deviceId, frame))
    }
}
