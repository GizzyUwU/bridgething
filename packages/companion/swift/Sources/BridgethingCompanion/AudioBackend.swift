import BridgethingSchema
import Foundation

#if canImport(AVFoundation)
    import AVFoundation
#endif

public protocol AudioBackend: Sendable {
    func setVolume(_ level: Float) async
    func setMute(_ muted: Bool) async
    func volumeUp() async
    func volumeDown() async
    func muteToggle() async
    func speak(id: UUID, text: String, voice: String?, onStart: @escaping @Sendable () -> Void) async -> Bool
    func cancel(id: UUID) async
    func cancelAll() async
    func playEarcon(name: String) async -> Bool
}

#if canImport(AVFoundation)
    public final class AvAudioBackend: AudioBackend, @unchecked Sendable {
        private let synth = AVSpeechSynthesizer()
        private let delegate = SpeechDelegate()
        private let earconBundle: Bundle
        private let playerStore = PlayerStore()

        public init(earconBundle: Bundle = .main) {
            self.earconBundle = earconBundle
            synth.delegate = delegate
        }

        public func setVolume(_ level: Float) async {}
        public func setMute(_ muted: Bool) async {}
        public func volumeUp() async {}
        public func volumeDown() async {}
        public func muteToggle() async {}

        public func speak(
            id: UUID,
            text: String,
            voice: String?,
            onStart: @escaping @Sendable () -> Void
        ) async -> Bool {
            #if os(iOS)
                CompanionAudioSession.activateMixedPlayback()
            #endif
            let utterance = AVSpeechUtterance(string: text)
            if let voice {
                utterance.voice = AVSpeechSynthesisVoice(identifier: voice) ?? AVSpeechSynthesisVoice(language: voice)
            }
            let deadline = Self.speakDeadline(for: text)
            return await withCheckedContinuation { (cont: CheckedContinuation<Bool, Never>) in
                delegate.register(utterance, deadlineNanos: deadline, onStart: onStart) { completed in cont.resume(returning: completed) }
                synth.speak(utterance)
            }
        }

        private static func speakDeadline(for text: String) -> UInt64 {
            let perChar = 0.12
            let seconds = max(15.0, Double(text.count) * perChar + 10.0)
            return UInt64(seconds * 1_000_000_000)
        }

        public func cancel(id: UUID) async {
            synth.stopSpeaking(at: .immediate)
        }

        public func cancelAll() async {
            synth.stopSpeaking(at: .immediate)
        }

        public func playEarcon(name: String) async -> Bool {
            #if os(iOS)
                CompanionAudioSession.activateMixedPlayback()
            #endif
            let exts = ["wav", "caf", "aiff", "m4a", "mp3"]
            let bundle = earconBundle
            let url = exts.lazy
                .compactMap { bundle.url(forResource: name, withExtension: $0, subdirectory: "earcons") }
                .first
            guard let url, let player = try? AVAudioPlayer(contentsOf: url) else { return false }
            playerStore.retainWhilePlaying(player)
            return player.play()
        }
    }

    final class SpeechDelegate: NSObject, AVSpeechSynthesizerDelegate, @unchecked Sendable {
        private struct Entry {
            let onStart: @Sendable () -> Void
            let onFinish: @Sendable (Bool) -> Void
        }

        private let lock = NSLock()
        private var entries: [ObjectIdentifier: Entry] = [:]
        private var watchdogs: [ObjectIdentifier: Task<Void, Never>] = [:]

        func register(
            _ utterance: AVSpeechUtterance,
            deadlineNanos: UInt64 = 0,
            onStart: @escaping @Sendable () -> Void,
            onFinish: @escaping @Sendable (Bool) -> Void
        ) {
            let key = ObjectIdentifier(utterance)
            lock.lock()
            entries[key] = Entry(onStart: onStart, onFinish: onFinish)
            if deadlineNanos > 0 {
                watchdogs[key] = Task { [weak self] in
                    try? await Task.sleep(nanoseconds: deadlineNanos)
                    self?.resolve(key, completed: false)
                }
            }
            lock.unlock()
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didStart utterance: AVSpeechUtterance) {
            lock.lock()
            let entry = entries[ObjectIdentifier(utterance)]
            lock.unlock()
            entry?.onStart()
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
            resolve(ObjectIdentifier(utterance), completed: true)
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
            resolve(ObjectIdentifier(utterance), completed: false)
        }

        func resolve(_ key: ObjectIdentifier, completed: Bool) {
            lock.lock()
            let entry = entries.removeValue(forKey: key)
            watchdogs.removeValue(forKey: key)?.cancel()
            lock.unlock()
            entry?.onFinish(completed)
        }
    }

    /// retains the player until the delegate fires; `play()` returns before playback ends.
    private final class PlayerStore: NSObject, AVAudioPlayerDelegate, @unchecked Sendable {
        private let lock = NSLock()
        private var players: [ObjectIdentifier: AVAudioPlayer] = [:]

        func retainWhilePlaying(_ player: AVAudioPlayer) {
            player.delegate = self
            lock.lock()
            players[ObjectIdentifier(player)] = player
            lock.unlock()
        }

        func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
            lock.lock()
            players.removeValue(forKey: ObjectIdentifier(player))
            lock.unlock()
        }
    }
#else
    public struct NoOpAudioBackend: AudioBackend {
        public init() {}
        public func setVolume(_ level: Float) async {}
        public func setMute(_ muted: Bool) async {}
        public func volumeUp() async {}
        public func volumeDown() async {}
        public func muteToggle() async {}
        public func speak(
            id: UUID,
            text: String,
            voice: String?,
            onStart: @escaping @Sendable () -> Void
        ) async -> Bool {
            onStart()
            return true
        }
        public func cancel(id: UUID) async {}
        public func cancelAll() async {}
        public func playEarcon(name: String) async -> Bool { false }
    }
#endif
