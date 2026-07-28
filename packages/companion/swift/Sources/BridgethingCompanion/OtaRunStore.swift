import BridgethingSchema
import Foundation

public enum OtaRunOutcome: String, Sendable, Equatable {
    case succeeded
    case failed
    case cancelled
}

public enum OtaRunPhase: String, Sendable, Equatable {
    case idle
    case downloading
    case streaming
    case verifying
    case writing
    case confirming
    case reboot
    case completed
    case failed
}

public struct OtaRun: Sendable, Equatable {
    public var runId: String
    public var deviceId: String
    public var kind: OtaKind
    public var phase: OtaRunPhase
    public var steps: [OtaPlanStep]
    public var stepId: Int
    public var startedAt: Date
    public var phaseStartedAt: Date
    public var stageReceived: UInt64?
    public var stageTotal: UInt64?
    public var ratePerSec: Double?
    public var dwlPercent: Int?
    public var outcome: OtaRunOutcome?
    public var error: String?
    public var releaseVersion: String?
    public var daemonVersion: String?
    public var imageVersion: String?
    public var webappId: String?
    public var webappName: String?
}

public struct OtaAvailable: Sendable, Equatable {
    public var deviceId: String
    public var releaseVersion: String?
    public var daemonVersion: String?
    public var imageVersion: String?
}

public struct OtaPollStatus: Sendable, Equatable {
    public var lastPolledAt: String?
    public var error: String?

    public init(lastPolledAt: String? = nil, error: String? = nil) {
        self.lastPolledAt = lastPolledAt
        self.error = error
    }
}

public enum OtaStoreChange: Sendable {
    case run(OtaRun)
    case available(OtaAvailable)
    case poll(OtaPollStatus)
}

final class OtaRunStore: @unchecked Sendable {
    private let lock = NSLock()
    private var runsByDevice: [String: OtaRun] = [:]
    private var availableByDevice: [String: OtaAvailable] = [:]
    private var poll = OtaPollStatus()

    func runs() -> [OtaRun] {
        lock.lock(); defer { lock.unlock() }
        return Array(runsByDevice.values)
    }

    func available() -> [OtaAvailable] {
        lock.lock(); defer { lock.unlock() }
        return Array(availableByDevice.values)
    }

    func pollStatus() -> OtaPollStatus {
        lock.lock(); defer { lock.unlock() }
        return poll
    }

    func dismiss(deviceId: String) -> OtaRun? {
        lock.lock(); defer { lock.unlock() }
        guard let run = runsByDevice[deviceId], run.outcome != nil else { return nil }
        runsByDevice.removeValue(forKey: deviceId)
        var cleared = run
        cleared.phase = .idle
        return cleared
    }

    func interrupt(deviceId: String) -> OtaRun? {
        lock.lock(); defer { lock.unlock() }
        guard var run = runsByDevice[deviceId], run.outcome == nil else { return nil }
        guard run.phase != .reboot, run.phase != .confirming else { return nil }
        run.phase = .failed
        run.outcome = .failed
        run.error = "the device disconnected mid-update"
        runsByDevice[deviceId] = run
        return run
    }

    func noteMeta(deviceId: String, daemonVersion: String, imageVersion: String) -> OtaRun? {
        lock.lock(); defer { lock.unlock() }
        guard let run = runsByDevice[deviceId] else { return nil }
        let wantsDaemon = run.daemonVersion
        let wantsImage = run.imageVersion
        let daemonOk = wantsDaemon == nil || wantsDaemon == daemonVersion
        let imageOk = wantsImage == nil || wantsImage == imageVersion
        guard daemonOk, imageOk else { return nil }
        let targeted = wantsDaemon != nil || wantsImage != nil
        guard targeted || run.outcome == .succeeded else { return nil }
        runsByDevice.removeValue(forKey: deviceId)
        var cleared = run
        cleared.phase = .idle
        cleared.outcome = .succeeded
        cleared.error = nil
        return cleared
    }

    func openRunKind(deviceId: String) -> OtaKind? {
        lock.lock(); defer { lock.unlock() }
        guard let run = runsByDevice[deviceId], run.outcome == nil else { return nil }
        return run.kind
    }

    func annotateWebapp(deviceId: String, webappId: String?, webappName: String?) -> OtaRun? {
        lock.lock(); defer { lock.unlock() }
        guard var run = runsByDevice[deviceId] else { return nil }
        run.webappId = webappId
        run.webappName = webappName
        runsByDevice[deviceId] = run
        return run
    }

    func ingest(_ event: OtaPollEvent, now: Date) -> [OtaStoreChange] {
        lock.lock(); defer { lock.unlock() }
        switch event {
        case let .manifestPolled(updatedAt):
            poll = OtaPollStatus(lastPolledAt: updatedAt, error: nil)
            return [.poll(poll)]

        case let .manifestPollFailed(reason):
            poll = OtaPollStatus(lastPolledAt: poll.lastPolledAt, error: reason)
            return [.poll(poll)]

        case let .updateAvailable(deviceId, release, daemonVersion, imageVersion):
            let entry = OtaAvailable(
                deviceId: deviceId,
                releaseVersion: release,
                daemonVersion: daemonVersion,
                imageVersion: imageVersion
            )
            availableByDevice[deviceId] = entry
            return [.available(entry)]

        case let .planned(deviceId, kind, release, daemonVersion, imageVersion, steps):
            let run = OtaRun(
                runId: UUID().uuidString,
                deviceId: deviceId,
                kind: kind,
                phase: .idle,
                steps: steps,
                stepId: steps.first?.id ?? 0,
                startedAt: now,
                phaseStartedAt: now,
                releaseVersion: release.isEmpty ? nil : release,
                daemonVersion: daemonVersion.isEmpty ? nil : daemonVersion,
                imageVersion: imageVersion.isEmpty ? nil : imageVersion
            )
            runsByDevice[deviceId] = run
            return [.run(run)]

        case let .progress(deviceId, kind, stepId, snapshot):
            guard var run = runsByDevice[deviceId] else { return [] }
            let before = run.phase
            run.kind = kind
            if run.steps.isEmpty || run.steps.contains(where: { $0.id == stepId }) {
                run.stepId = stepId
            }
            apply(snapshot: snapshot, to: &run)
            if run.phase != before { run.phaseStartedAt = now }
            runsByDevice[deviceId] = run
            return [.run(run)]

        case let .updated(deviceId, _, version):
            guard var run = runsByDevice[deviceId] else { return [] }
            run.phase = .completed
            run.outcome = .succeeded
            run.error = nil
            run.stageReceived = nil
            run.stageTotal = nil
            run.ratePerSec = nil
            run.dwlPercent = nil
            if run.releaseVersion == nil, !version.isEmpty { run.releaseVersion = version }
            runsByDevice[deviceId] = run
            availableByDevice.removeValue(forKey: deviceId)
            return [.run(run), .available(OtaAvailable(deviceId: deviceId))]

        case let .failed(deviceId, kind, reason):
            var run = runsByDevice[deviceId] ?? OtaRun(
                runId: UUID().uuidString,
                deviceId: deviceId,
                kind: kind,
                phase: .failed,
                steps: [],
                stepId: 0,
                startedAt: now,
                phaseStartedAt: now
            )
            run.phase = .failed
            run.outcome = reason == cancelledReason ? .cancelled : .failed
            run.error = reason
            run.stageReceived = nil
            run.stageTotal = nil
            run.ratePerSec = nil
            runsByDevice[deviceId] = run
            return [.run(run)]
        }
    }

    private func apply(snapshot: OtaPhaseSnapshot, to run: inout OtaRun) {
        switch snapshot {
        case .idle:
            run.phase = .idle
        case let .downloading(_, received, total, rate):
            run.phase = .downloading
            run.stageReceived = received
            run.stageTotal = total
            run.ratePerSec = rate
        case let .streaming(_, sent, total, rate, _):
            run.phase = .streaming
            run.stageReceived = sent
            run.stageTotal = total
            run.ratePerSec = rate
        case let .applying(phase, _, dwlPercent, dwlBytes):
            run.phase = switch phase {
            case .streaming: .streaming
            case .verifying: .verifying
            case .writing: .writing
            case .confirming: .confirming
            case .reboot: .reboot
            }
            run.dwlPercent = dwlPercent
            run.stageReceived = dwlPercent < 100 && dwlBytes > 0 ? dwlBytes : nil
            run.stageTotal = nil
        case .staged:
            run.phase = .writing
            run.stageReceived = nil
            run.stageTotal = nil
        case .completed:
            run.phase = .completed
        case let .failed(reason):
            run.phase = .failed
            run.error = reason
        }
    }
}

let cancelledReason = "cancelled"
