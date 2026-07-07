import Foundation

struct TransferStalled: Error, LocalizedError {
    var errorDescription: String? { "transfer stalled: fragment acks stopped" }
}

actor TransferAckWindow {
    private var received: [UUID: UInt32] = [:]
    private var waiters: [UUID: [(id: UUID, continuation: CheckedContinuation<Bool, Never>)]] = [:]

    func note(transferId: UUID, received bytes: UInt32) {
        let prior = received[transferId] ?? 0
        guard bytes > prior else { return }
        received[transferId] = bytes
        for (_, continuation) in waiters.removeValue(forKey: transferId) ?? [] {
            continuation.resume(returning: true)
        }
    }

    func receivedBytes(_ transferId: UUID) -> UInt32 {
        received[transferId] ?? 0
    }

    func finish(_ transferId: UUID) {
        received.removeValue(forKey: transferId)
        for (_, continuation) in waiters.removeValue(forKey: transferId) ?? [] {
            continuation.resume(returning: false)
        }
    }

    func waitForProgress(_ transferId: UUID, beyond prior: UInt32, timeoutSeconds: Double) async -> Bool {
        if (received[transferId] ?? 0) > prior { return true }
        let waiterId = UUID()
        let timeout = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(timeoutSeconds * 1_000_000_000))
            guard !Task.isCancelled else { return }
            await self?.expire(transferId: transferId, waiterId: waiterId)
        }
        let progressed = await withTaskCancellationHandler {
            await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
                waiters[transferId, default: []].append((waiterId, continuation))
            }
        } onCancel: {
            Task { [weak self] in await self?.expire(transferId: transferId, waiterId: waiterId) }
        }
        timeout.cancel()
        return progressed
    }

    private func expire(transferId: UUID, waiterId: UUID) {
        guard var list = waiters[transferId],
              let index = list.firstIndex(where: { $0.id == waiterId })
        else { return }
        let (_, continuation) = list.remove(at: index)
        waiters[transferId] = list.isEmpty ? nil : list
        continuation.resume(returning: false)
    }
}
