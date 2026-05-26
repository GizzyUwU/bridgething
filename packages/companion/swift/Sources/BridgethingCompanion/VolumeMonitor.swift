#if os(iOS)
    import AVFoundation
    import Foundation

    /// Host audio output volume + mute snapshot via `AVAudioSession`.
    /// iOS doesn't expose a system-wide media-mute flag (the hardware
    /// switch only mutes ringer); `muted == true` means `outputVolume == 0`.
    public actor VolumeMonitor {
        public typealias Callback = @Sendable (Float, Bool) -> Void

        private let session = AVAudioSession.sharedInstance()
        private var observation: NSKeyValueObservation?

        public init() {}

        public func start(_ callback: @escaping Callback) async {
            stopObservation()
            // KVO callback fires on an arbitrary framework thread; the closure is Sendable.
            observation = session.observe(\.outputVolume, options: [.initial, .new]) { session, _ in
                let v = session.outputVolume
                callback(v, v == 0)
            }
        }

        public func stop() async {
            stopObservation()
        }

        public func snapshot() async -> (level: Float, muted: Bool)? {
            let v = session.outputVolume
            return (level: v, muted: v == 0)
        }

        private func stopObservation() {
            observation?.invalidate()
            observation = nil
        }
    }
#endif
