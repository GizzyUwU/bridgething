package com.bridgething.glue

import com.bridgething.schema.PlayUri
import com.bridgething.schema.QueueUri
import com.bridgething.schema.RepeatMode

/**
 * Control surface for the audible now-playing source. A [BridgethingGlue]
 * (which resolves provider uris) implements it, and so does a non-glue source
 * such as a foreign MediaSession (play/pause/skip only). The companion's hub
 * routes an inbound transport verb to whichever source is currently audible, so
 * a Pause/SkipNext lands on the app the user actually hears.
 *
 * Default impls throw [GlueError.NotImplemented]; a source overrides what it
 * supports. `play`/`queue` are uri-scheme work and only a glue resolves them.
 */
interface NowPlayingTransport {
    suspend fun play(uri: PlayUri): Unit = throw GlueError.NotImplemented
    suspend fun pause(): Unit = throw GlueError.NotImplemented
    suspend fun resume(): Unit = throw GlueError.NotImplemented
    suspend fun skipNext(): Unit = throw GlueError.NotImplemented
    suspend fun skipPrev(): Unit = throw GlueError.NotImplemented
    suspend fun skipToIndex(index: UInt): Unit = throw GlueError.NotImplemented
    suspend fun seekTo(positionMs: UInt): Unit = throw GlueError.NotImplemented
    suspend fun queue(req: QueueUri): Unit = throw GlueError.NotImplemented
    suspend fun setShuffle(on: Boolean): Unit = throw GlueError.NotImplemented
    suspend fun setRepeat(mode: RepeatMode): Unit = throw GlueError.NotImplemented
    suspend fun setSpeed(speed: Float): Unit = throw GlueError.NotImplemented
    suspend fun setCrossfade(durationMs: UInt?): Unit = throw GlueError.NotImplemented
}
