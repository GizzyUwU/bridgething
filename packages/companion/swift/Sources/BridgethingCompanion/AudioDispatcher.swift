import BridgethingGateway
import BridgethingGlue
import BridgethingSchema
import Foundation

public actor AudioDispatcher {
    private let backend: any AudioBackend
    private var tasks: [Task<Void, Never>] = []
    private var glueProvider: (@Sendable () async -> (any BridgethingGlue)?)?

    public init(backend: any AudioBackend) {
        self.backend = backend
    }

    public func setGlueProvider(_ provider: @escaping @Sendable () async -> (any BridgethingGlue)?) {
        glueProvider = provider
    }

    private func volumeGlue() async -> (any BridgethingGlue)? {
        guard let glue = await glueProvider?(), await glue.ownsVolume() else { return nil }
        return glue
    }

    public func start(gateway: BridgethingGateway) async {
        let backend = backend
        tasks.append(Task { [weak self] in
            for await _ in gateway.audio.volumeUp {
                guard let glue = await self?.volumeGlue() else { await backend.volumeUp(); continue }
                do { try await glue.volumeUp() } catch { await Self.reportRejected("volumeUp", error, gateway: gateway) }
            }
        })
        tasks.append(Task { [weak self] in
            for await _ in gateway.audio.volumeDown {
                guard let glue = await self?.volumeGlue() else { await backend.volumeDown(); continue }
                do { try await glue.volumeDown() } catch { await Self.reportRejected("volumeDown", error, gateway: gateway) }
            }
        })
        tasks.append(Task { [weak self] in
            for await (_, msg) in gateway.audio.setVolume {
                guard let glue = await self?.volumeGlue() else { await backend.setVolume(msg.level); continue }
                do { try await glue.setVolume(msg.level) } catch { await Self.reportRejected("setVolume", error, gateway: gateway) }
            }
        })
        tasks.append(Task { [weak self] in
            for await _ in gateway.audio.muteToggle {
                if await self?.volumeGlue() == nil { await backend.muteToggle() }
            }
        })
        tasks.append(Task { [weak self] in
            for await (_, msg) in gateway.audio.setMute {
                if await self?.volumeGlue() == nil { await backend.setMute(msg.muted) }
            }
        })
        tasks.append(Task { for await (_, msg) in gateway.audio.ttsCancel { await backend.cancel(id: msg.id) } })
        tasks.append(Task { for await _ in gateway.audio.ttsCancelAll { await backend.cancelAll() } })
        tasks.append(Task {
            for await (_, msg) in gateway.audio.earcon where await !backend.playEarcon(name: msg.name) {
                await Self.reportAudioError(
                    .unavailable(AudioErrorUnavailableInner(verb: "earcon")),
                    gateway: gateway
                )
            }
        })
        tasks.append(Task { [weak self] in
            for await (_, msg) in gateway.audio.tts {
                await self?.handleTts(msg, gateway: gateway)
            }
        })
    }

    public func stop() async {
        for task in tasks { task.cancel() }
        tasks.removeAll()
        await backend.cancelAll()
    }

    private static func reportRejected(_ verb: String, _ error: Error, gateway: BridgethingGateway) async {
        await reportAudioError(
            .actionRejected(AudioErrorActionRejectedInner(reason: "\(verb): \(error.localizedDescription)")),
            gateway: gateway
        )
    }

    private static func reportAudioError(_ error: AudioError, gateway: BridgethingGateway) async {
        try? await gateway.audio.errorEvent(AudioErrorReply(error: error))
    }

    private func handleTts(_ msg: Tts, gateway: BridgethingGateway) async {
        let id = msg.id
        let completed = await backend.speak(id: id, text: msg.text, voice: msg.voice) {
            Task { try? await gateway.audio.ttsStarted(TtsStarted(id: id)) }
        }
        try? await gateway.audio.ttsEnded(TtsEnded(id: id, completed: completed))
    }
}
