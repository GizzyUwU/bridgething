package com.bridgething.companion

import com.bridgething.glue.NowPlayingSink
import com.bridgething.glue.NowPlayingTransport
import com.bridgething.schema.MediaItem
import com.bridgething.schema.Playback
import com.bridgething.schema.PlaybackState
import com.bridgething.schema.PlayerOptions
import com.bridgething.schema.PlayerState
import com.bridgething.schema.RepeatMode

/**
 * Surfaces any foreign app's MediaSession now-playing (YouTube, Apple Music, podcasts) through the
 * companion's [NowPlayingHub] as the "system" source, gated on the notification-listener grant the app
 * already holds. It picks the audible (playing) session - skipping any whose package a glue already owns
 * so the active provider is not double-emitted - maps its metadata + playback state to a [PlayerState],
 * and submits to the hub. As the hub's registered transport for the system source, inbound play/pause/skip
 * route to that session's controls.
 */
internal class SystemMediaSource(
    private val gateway: MediaSessionGateway,
    private val sink: NowPlayingSink,
    private val providerPackages: () -> Set<String>,
) : NowPlayingTransport {
    private var handle: MediaSessionListenHandle? = null

    @Volatile private var audible: SystemMediaSession? = null

    private val lock = Any()
    private var lastSubmitted: Pair<String, SystemMediaSnapshot>? = null

    fun start() {
        if (handle != null) return
        handle = gateway.listen(::recompute)
        recompute()
    }

    fun stop() {
        handle?.stop()
        handle = null
        synchronized(lock) {
            audible = null
            lastSubmitted = null
        }
        sink.clearSource(SOURCE_ID)
    }

    /** Re-attach observation + recompute after a grant change (the listener could not register before it). */
    fun refresh() {
        handle?.stop()
        handle = gateway.listen(::recompute)
        recompute()
    }

    private fun recompute() = synchronized(lock) {
        val owned = providerPackages()
        val sessions = if (gateway.isAccessGranted) gateway.activeSessions() else emptyList()
        val picked = sessions.asSequence()
            .filter { it.packageName !in owned }
            .mapNotNull { s -> s.snapshot()?.let { s to it } }
            .firstOrNull { it.second.playing }
        if (picked == null) {
            audible = null
            if (lastSubmitted != null) {
                lastSubmitted = null
                sink.clearSource(SOURCE_ID)
            }
            return@synchronized
        }
        val (session, snap) = picked
        audible = session
        val key = session.packageName to snap
        if (key == lastSubmitted) return@synchronized
        lastSubmitted = key
        val hasItem = snap.title != null || snap.artist != null
        sink.submitPlayer(SOURCE_ID, toPlayerState(snap, session.packageName), session.packageName, hasItem)
    }

    private fun toPlayerState(snap: SystemMediaSnapshot, packageName: String): PlayerState {
        val track = if (snap.title == null && snap.artist == null) {
            null
        } else {
            val uri = "system:$packageName:${(snap.title ?: "").hashCode()}"
            MediaItem(
                uri = uri,
                persistentId = uri,
                title = snap.title,
                album = snap.album,
                albumUri = null,
                albumArtist = null,
                artist = snap.artist,
                artistUri = null,
                liked = null,
                artworkId = null,
                durationMs = snap.durationMs?.takeIf { it > 0L }?.coerceAtMost(UInt.MAX_VALUE.toLong())?.toUInt(),
                mediaTypes = null,
                trackNumber = null,
                trackCount = null,
                isLikeSupported = null,
                isBanSupported = null,
                isBanned = null,
                chapterCount = null,
            )
        }
        val playback = Playback(
            state = if (snap.playing) PlaybackState.Playing else PlaybackState.Paused,
            positionMs = snap.positionMs.coerceIn(0L, UInt.MAX_VALUE.toLong()).toUInt(),
            shuffle = false,
            shuffleMode = null,
            repeat = RepeatMode.Off,
            queueIndex = null,
            queueCount = null,
            queueChapterIndex = null,
            setElapsedTimeAvailable = snap.canSeek,
            queueListAvail = null,
            appleMusicRadioAd = null,
        )
        return PlayerState(
            track = track,
            playback = playback,
            queue = emptyList(),
            options = PlayerOptions(speed = 1.0f, crossfade_ms = null),
            context = null,
        )
    }

    override suspend fun pause() { audible?.pause() }
    override suspend fun resume() { audible?.play() }
    override suspend fun skipNext() { audible?.skipNext() }
    override suspend fun skipPrev() { audible?.skipPrev() }
    override suspend fun seekTo(positionMs: UInt) { audible?.seekTo(positionMs.toLong()) }

    companion object {
        const val SOURCE_ID = "system"
    }
}
