package com.bridgething.companion

import com.bridgething.glue.AssetBytes
import com.bridgething.glue.NowPlayingSink
import com.bridgething.glue.NowPlayingTransport
import com.bridgething.schema.MediaItem
import com.bridgething.schema.Playback
import com.bridgething.schema.PlaybackContext
import com.bridgething.schema.PlaybackState
import com.bridgething.schema.PlayerOptions
import com.bridgething.schema.PlayerState
import com.bridgething.schema.QueueItem
import com.bridgething.schema.QueueSnapshot
import com.bridgething.schema.RepeatMode
import com.bridgething.schema.ShuffleMode

internal class SystemMediaSource(
    private val gateway: MediaSessionGateway,
    private val sink: NowPlayingSink,
    private val providerPackages: () -> Set<String>,
) : NowPlayingTransport {
    private var handle: MediaSessionListenHandle? = null

    @Volatile private var audible: SystemMediaSession? = null

    @Volatile private var upcoming: List<SystemMediaQueueEntry> = emptyList()

    private val lock = Any()
    private var lastPlayer: Pair<String, SystemMediaSnapshot>? = null
    private var lastQueue: Pair<String, List<SystemMediaQueueEntry>>? = null

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
            upcoming = emptyList()
            lastPlayer = null
            lastQueue = null
        }
        sink.clearSource(SOURCE_ID)
    }

    fun refresh() {
        handle?.stop()
        handle = gateway.listen(::recompute)
        recompute()
    }

    suspend fun asset(id: String): AssetBytes? {
        if (!id.startsWith(ASSET_ID_PREFIX)) return null
        val token = id.removePrefix(ASSET_ID_PREFIX)
        val art = audible?.art(token) ?: return null
        return AssetBytes(bytes = art.bytes, mime = art.mime)
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
            upcoming = emptyList()
            if (lastPlayer != null) {
                lastPlayer = null
                lastQueue = null
                sink.clearSource(SOURCE_ID)
            }
            return@synchronized
        }
        val (session, snap) = picked
        audible = session
        val nextUp = upcomingWindow(snap)
        upcoming = nextUp

        val playerKey = session.packageName to snap.copy(queue = emptyList(), activeQueueId = null, positionAgeMs = null)
        if (playerKey != lastPlayer) {
            lastPlayer = playerKey
            val hasItem = snap.title != null || snap.artist != null
            sink.submitPlayer(SOURCE_ID, toPlayerState(snap, session.packageName), session.packageName, hasItem)
        }
        val queueKey = session.packageName to nextUp
        if (queueKey != lastQueue) {
            lastQueue = queueKey
            sink.submitQueue(SOURCE_ID, toQueueSnapshot(nextUp, session.packageName))
        }
    }

    private fun upcomingWindow(snap: SystemMediaSnapshot): List<SystemMediaQueueEntry> {
        if (snap.queue.isEmpty()) return emptyList()
        val activeIdx = snap.activeQueueId?.let { id -> snap.queue.indexOfFirst { it.queueId == id } } ?: -1
        return if (activeIdx < 0) snap.queue else snap.queue.drop(activeIdx + 1)
    }

    private fun toQueueSnapshot(entries: List<SystemMediaQueueEntry>, packageName: String): QueueSnapshot {
        val items = entries.map { entry ->
            val uri = "system:$packageName:q${entry.queueId}"
            QueueItem(
                uri = uri,
                title = entry.title,
                artist = entry.subtitle,
                artistUri = null,
                album = null,
                albumUri = null,
                artworkId = entry.artToken?.let { "$ASSET_ID_PREFIX$it" },
                durationMs = null,
                persistentId = uri,
                queued = false,
            )
        }
        return QueueSnapshot(order = items.map { it.uri }, items = items)
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
                liked = if (snap.likeSupported) snap.liked else null,
                artworkId = snap.artToken?.let { "$ASSET_ID_PREFIX$it" },
                durationMs = snap.durationMs?.takeIf { it > 0L }?.coerceAtMost(UInt.MAX_VALUE.toLong())?.toUInt(),
                mediaTypes = null,
                trackNumber = null,
                trackCount = null,
                isLikeSupported = if (snap.likeSupported) true else null,
                isBanSupported = null,
                isBanned = null,
                chapterCount = null,
            )
        }
        val playback = Playback(
            state = if (snap.playing) PlaybackState.Playing else PlaybackState.Paused,
            positionMs = snap.positionMs.coerceIn(0L, UInt.MAX_VALUE.toLong()).toUInt(),
            positionAgeMs = snap.positionAgeMs?.coerceIn(0L, UInt.MAX_VALUE.toLong())?.toUInt(),
            shuffle = snap.shuffle ?: false,
            shuffleMode = snap.shuffle?.let { if (it) ShuffleMode.Songs else ShuffleMode.Off },
            repeat = snap.repeat ?: RepeatMode.Off,
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
            options = PlayerOptions(speed = snap.speed ?: 1.0f, crossfadeMs = null),
            context = snap.queueTitle?.let { PlaybackContext(uri = "system:$packageName:context", name = it) },
        )
    }

    override suspend fun pause() { audible?.pause() }
    override suspend fun resume() { audible?.play() }
    override suspend fun skipNext() { audible?.skipNext() }
    override suspend fun skipPrev() { audible?.skipPrev() }
    override suspend fun seekTo(positionMs: UInt) { audible?.seekTo(positionMs.toLong()) }
    override suspend fun setShuffle(on: Boolean) { audible?.setShuffle(on) }
    override suspend fun setRepeat(mode: RepeatMode) { audible?.setRepeat(mode) }
    override suspend fun setSpeed(speed: Float) { audible?.setSpeed(speed) }

    override suspend fun skipToIndex(index: UInt) {
        val entry = upcoming.getOrNull(index.toInt()) ?: return
        audible?.skipToQueueItem(entry.queueId)
    }

    fun owns(uri: String): Boolean = uri.startsWith("system:")

    fun setLiked(liked: Boolean) { audible?.setLiked(liked) }

    fun toggleLiked() {
        val current = synchronized(lock) { lastPlayer?.second?.liked } ?: false
        audible?.setLiked(!current)
    }

    companion object {
        const val SOURCE_ID = "system"
        const val ASSET_ID_PREFIX = "system-art:"
    }
}
