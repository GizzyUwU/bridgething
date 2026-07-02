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
                if let glue = await self?.volumeGlue() { try? await glue.volumeUp() } else { await backend.volumeUp() }
            }
        })
        tasks.append(Task { [weak self] in
            for await _ in gateway.audio.volumeDown {
                if let glue = await self?.volumeGlue() { try? await glue.volumeDown() } else { await backend.volumeDown() }
            }
        })
        tasks.append(Task { [weak self] in
            for await (_, msg) in gateway.audio.setVolume {
                if let glue = await self?.volumeGlue() { try? await glue.setVolume(msg.level) } else { await backend.setVolume(msg.level) }
            }
        })
        tasks.append(Task { [weak self] in
            for await _ in gateway.audio.muteToggle {
                // connect has no mute surface; swallow rather than mute the phone
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
        tasks.append(Task { for await (_, msg) in gateway.audio.earcon { _ = await backend.playEarcon(name: msg.name) } })
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

    private func handleTts(_ msg: Tts, gateway: BridgethingGateway) async {
        let id = msg.id
        let completed = await backend.speak(id: id, text: msg.text, voice: msg.voice) {
            Task { try? await gateway.audio.ttsStarted(TtsStarted(id: id)) }
        }
        try? await gateway.audio.ttsEnded(TtsEnded(id: id, completed: completed))
    }
}
