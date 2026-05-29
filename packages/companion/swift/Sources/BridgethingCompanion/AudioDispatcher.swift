import BridgethingGateway
import BridgethingSchema
import Foundation

public actor AudioDispatcher {
    private let backend: any AudioBackend
    private var tasks: [Task<Void, Never>] = []

    public init(backend: any AudioBackend) {
        self.backend = backend
    }

    public func start(gateway: BridgethingGateway) async {
        let backend = backend
        tasks.append(Task { for await _ in gateway.audio.volumeUp { await backend.volumeUp() } })
        tasks.append(Task { for await _ in gateway.audio.volumeDown { await backend.volumeDown() } })
        tasks.append(Task { for await (_, msg) in gateway.audio.setVolume { await backend.setVolume(msg.level) } })
        tasks.append(Task { for await _ in gateway.audio.muteToggle { await backend.muteToggle() } })
        tasks.append(Task { for await (_, msg) in gateway.audio.setMute { await backend.setMute(msg.muted) } })
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
