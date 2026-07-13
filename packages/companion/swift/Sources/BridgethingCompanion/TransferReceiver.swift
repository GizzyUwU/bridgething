import BridgethingGateway
import BridgethingSchema
import Foundation
#if canImport(CryptoKit)
    import CryptoKit
#endif

enum TransferReceiverError: Error, CustomStringConvertible {
    case notRegistered
    case timeout
    case tooLarge(totalSize: UInt32)
    case overflow
    case gap(expected: UInt32, got: UInt32)
    case shaMismatch(expected: String, got: String)
    case abandoned(reason: String)
    case cryptoUnavailable
    case stopped

    var description: String {
        switch self {
        case .notRegistered: "transfer was never registered"
        case .timeout: "transfer timed out before completing"
        case let .tooLarge(total): "transfer of \(total) bytes exceeds the 1 MiB cap"
        case .overflow: "fragment ran past the declared total size"
        case let .gap(expected, got): "non-contiguous fragment: expected offset \(expected), got \(got)"
        case let .shaMismatch(expected, got): "sha256 mismatch: expected \(expected), got \(got)"
        case let .abandoned(reason): "daemon abandoned the transfer: \(reason)"
        case .cryptoUnavailable: "CryptoKit unavailable on this platform"
        case .stopped: "receiver stopped"
        }
    }
}

actor TransferReceiver {
    private static let maxBytes = 1 * 1024 * 1024
    private static let ackInterval: UInt32 = 16 * 1024
    private static let bufferCap = 512 * 1024
    private static let bufferTtl: Duration = .seconds(5)

    private struct Pending {
        let deviceId: String
        let ref: TransferRef
        var buffer: Data
        var lastAcked: UInt32
        var waiter: CheckedContinuation<Data, Error>?
        var terminal: Result<Data, Error>?
        var timeout: Task<Void, Never>?
    }

    private struct Buffered {
        var fragments: [(offset: UInt32, bytes: Data)]
        var bytes: Int
        var ttl: Task<Void, Never>?
    }

    private var gateway: BridgethingGateway?
    private var pending: [UUID: Pending] = [:]
    private var buffer: [UUID: Buffered] = [:]
    private var bufferedBytesTotal = 0
    private var fragmentTask: Task<Void, Never>?
    private var abandonTask: Task<Void, Never>?

    func start(gateway: BridgethingGateway) {
        self.gateway = gateway
        fragmentTask?.cancel()
        fragmentTask = Task { [weak self] in
            for await (deviceId, frag) in gateway.transfer.fragment {
                await self?.handleFragment(deviceId: deviceId, frag)
            }
        }
        abandonTask?.cancel()
        abandonTask = Task { [weak self] in
            for await (_, ab) in gateway.transfer.abandon {
                await self?.handleAbandon(ab)
            }
        }
    }

    func stop() {
        fragmentTask?.cancel()
        fragmentTask = nil
        abandonTask?.cancel()
        abandonTask = nil
        for id in Array(pending.keys) { fail(id, .stopped) }
        for b in buffer.values { b.ttl?.cancel() }
        buffer.removeAll()
        bufferedBytesTotal = 0
    }

    func register(deviceId: String, ref: TransferRef) async {
        guard pending[ref.id] == nil else { return }
        if ref.totalSize == 0 {
            pending[ref.id] = Pending(
                deviceId: deviceId, ref: ref, buffer: Data(), lastAcked: 0,
                waiter: nil, terminal: .success(Data()), timeout: nil
            )
            return
        }
        if ref.totalSize > UInt32(Self.maxBytes) {
            pending[ref.id] = Pending(
                deviceId: deviceId, ref: ref, buffer: Data(), lastAcked: 0,
                waiter: nil, terminal: .failure(TransferReceiverError.tooLarge(totalSize: ref.totalSize)), timeout: nil
            )
            return
        }
        pending[ref.id] = Pending(
            deviceId: deviceId, ref: ref, buffer: Data(), lastAcked: 0,
            waiter: nil, terminal: nil, timeout: nil
        )
        guard let held = buffer.removeValue(forKey: ref.id) else { return }
        held.ttl?.cancel()
        bufferedBytesTotal -= held.bytes
        var acks: [UInt32] = []
        for f in held.fragments {
            if pending[ref.id]?.terminal != nil { break }
            if let ack = ingest(ref.id, offset: f.offset, bytes: f.bytes) { acks.append(ack) }
        }
        for ack in acks { await sendAck(deviceId: deviceId, id: ref.id, received: ack) }
    }

    func collect(_ id: UUID, timeout: Duration) async throws -> Data {
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Data, Error>) in
            guard var p = pending[id] else {
                cont.resume(throwing: TransferReceiverError.notRegistered)
                return
            }
            if let terminal = p.terminal {
                pending.removeValue(forKey: id)
                cont.resume(with: terminal)
                return
            }
            p.waiter = cont
            p.timeout = Task { [weak self] in
                try? await Task.sleep(for: timeout)
                await self?.timedOut(id)
            }
            pending[id] = p
        }
    }

    private func handleFragment(deviceId: String, _ frag: TransferFragment) async {
        guard let sinkDeviceId = pending[frag.transferId]?.deviceId else {
            bufferUnregistered(frag)
            return
        }
        if let ack = ingest(frag.transferId, offset: frag.offset, bytes: frag.bytes) {
            await sendAck(deviceId: sinkDeviceId, id: frag.transferId, received: ack)
        }
    }

    private func handleAbandon(_ ab: TransferAbandon) {
        if let held = buffer.removeValue(forKey: ab.transferId) {
            held.ttl?.cancel()
            bufferedBytesTotal -= held.bytes
        }
        guard pending[ab.transferId] != nil else { return }
        fail(ab.transferId, .abandoned(reason: ab.reason))
    }

    private func ingest(_ id: UUID, offset: UInt32, bytes: Data) -> UInt32? {
        guard var p = pending[id], p.terminal == nil else { return nil }
        guard offset == UInt32(p.buffer.count) else {
            fail(id, .gap(expected: UInt32(p.buffer.count), got: offset))
            return nil
        }
        let newCount = p.buffer.count + bytes.count
        guard newCount <= Self.maxBytes, UInt64(newCount) <= UInt64(p.ref.totalSize) else {
            fail(id, .overflow)
            return nil
        }
        p.buffer.append(bytes)
        pending[id] = p
        let received = UInt32(newCount)
        if received == p.ref.totalSize {
            if let err = shaError(p.buffer, p.ref.sha256) {
                fail(id, err)
            } else {
                complete(id, p.buffer)
            }
            return received
        }
        if received - p.lastAcked >= Self.ackInterval {
            p.lastAcked = received
            pending[id] = p
            return received
        }
        return nil
    }

    private func complete(_ id: UUID, _ data: Data) {
        guard var p = pending[id] else { return }
        p.timeout?.cancel()
        if let waiter = p.waiter {
            pending.removeValue(forKey: id)
            waiter.resume(returning: data)
        } else {
            p.terminal = .success(data)
            p.timeout = nil
            pending[id] = p
        }
    }

    private func fail(_ id: UUID, _ error: TransferReceiverError) {
        guard var p = pending[id] else { return }
        p.timeout?.cancel()
        if let waiter = p.waiter {
            pending.removeValue(forKey: id)
            waiter.resume(throwing: error)
        } else {
            p.terminal = .failure(error)
            p.timeout = nil
            pending[id] = p
        }
    }

    private func timedOut(_ id: UUID) {
        guard let p = pending[id], p.terminal == nil, let waiter = p.waiter else { return }
        pending.removeValue(forKey: id)
        waiter.resume(throwing: TransferReceiverError.timeout)
    }

    private func bufferUnregistered(_ frag: TransferFragment) {
        guard bufferedBytesTotal + frag.bytes.count <= Self.bufferCap else { return }
        let id = frag.transferId
        var held = buffer[id] ?? Buffered(fragments: [], bytes: 0, ttl: nil)
        held.fragments.append((frag.offset, frag.bytes))
        held.bytes += frag.bytes.count
        bufferedBytesTotal += frag.bytes.count
        held.ttl?.cancel()
        held.ttl = Task { [weak self] in
            try? await Task.sleep(for: Self.bufferTtl)
            await self?.evictBuffered(id)
        }
        buffer[id] = held
    }

    private func evictBuffered(_ id: UUID) {
        guard let held = buffer.removeValue(forKey: id) else { return }
        bufferedBytesTotal -= held.bytes
    }

    private func sendAck(deviceId: String, id: UUID, received: UInt32) async {
        guard let gateway else { return }
        try? await gateway.device(deviceId).transfer.ack(TransferAck(transferId: id, received: received))
    }

    private func shaError(_ data: Data, _ expected: String?) -> TransferReceiverError? {
        guard let expected else { return nil }
        #if canImport(CryptoKit)
            let got = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
            guard got.caseInsensitiveCompare(expected) == .orderedSame else {
                return .shaMismatch(expected: expected, got: got)
            }
            return nil
        #else
            return .cryptoUnavailable
        #endif
    }
}
