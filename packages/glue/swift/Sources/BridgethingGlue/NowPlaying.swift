import BridgethingSchema

public protocol NowPlayingTransport: Sendable {
    func play(_ uri: PlayUri) async throws
    func queue(_ req: QueueUri) async throws
    func pause() async throws
    func resume() async throws
    func skipNext() async throws
    func skipPrev() async throws
    func skipToIndex(_ index: UInt32) async throws
    func seekTo(_ ms: UInt32) async throws
    func setShuffle(_ on: Bool) async throws
    func setRepeat(_ mode: BridgethingSchema.RepeatMode) async throws
    func setSpeed(_ speed: Float) async throws
    func setCrossfade(_ durationMs: UInt32?) async throws
}

public extension NowPlayingTransport {
    func play(_: PlayUri) async throws { throw GlueError.notImplemented }
    func queue(_: QueueUri) async throws { throw GlueError.notImplemented }
    func pause() async throws { throw GlueError.notImplemented }
    func resume() async throws { throw GlueError.notImplemented }
    func skipNext() async throws { throw GlueError.notImplemented }
    func skipPrev() async throws { throw GlueError.notImplemented }
    func skipToIndex(_: UInt32) async throws { throw GlueError.notImplemented }
    func seekTo(_: UInt32) async throws { throw GlueError.notImplemented }
    func setShuffle(_: Bool) async throws { throw GlueError.notImplemented }
    func setRepeat(_: BridgethingSchema.RepeatMode) async throws { throw GlueError.notImplemented }
    func setSpeed(_: Float) async throws { throw GlueError.notImplemented }
    func setCrossfade(_: UInt32?) async throws { throw GlueError.notImplemented }
}

public protocol NowPlayingSink: Sendable {
    func submitPlayer(
        sourceId: String,
        snapshot: PlayerState,
        appBundle: String,
        hasItem: Bool,
        wantsVolume: Bool
    )
    func submitQueue(sourceId: String, queue: QueueSnapshot)
    func submitTargets(sourceId: String, targets: PlaybackTargets)
    func clearSource(sourceId: String)
}
