package com.bridgething.companion

import android.content.ComponentName
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadata
import android.media.Rating
import android.media.session.MediaController
import android.media.session.MediaSession
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.support.v4.media.session.MediaControllerCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationManagerCompat
import com.bridgething.schema.RepeatMode
import java.io.ByteArrayOutputStream
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap

public class AndroidMediaSessionGateway(
    context: Context,
    private val notificationListener: ComponentName,
) : MediaSessionGateway {
    private val appContext = context.applicationContext
    private val manager = appContext.getSystemService(Context.MEDIA_SESSION_SERVICE) as MediaSessionManager
    private val thread = HandlerThread("bridgething-media-sessions").apply { start() }
    private val handler = Handler(thread.looper)
    private val compatByToken = ConcurrentHashMap<MediaSession.Token, MediaControllerCompat>()

    override val isAccessGranted: Boolean
        get() = NotificationManagerCompat.getEnabledListenerPackages(appContext).contains(appContext.packageName)

    override fun activeSessions(): List<SystemMediaSession> =
        runCatching { manager.getActiveSessions(notificationListener) }
            .getOrDefault(emptyList())
            .map { AndroidSystemMediaSession(appContext, it, compatFor(it)) }

    private fun compatFor(controller: MediaController): MediaControllerCompat? =
        runCatching {
            compatByToken.getOrPut(controller.sessionToken) {
                MediaControllerCompat(appContext, MediaSessionCompat.Token.fromToken(controller.sessionToken))
            }
        }.getOrNull()

    override fun listen(onChanged: () -> Unit): MediaSessionListenHandle {
        val lock = Any()
        val perController = mutableListOf<Pair<MediaController, MediaController.Callback>>()
        val perCompat = mutableListOf<Pair<MediaControllerCompat, MediaControllerCompat.Callback>>()

        fun attachPerController(controllers: List<MediaController>) = synchronized(lock) {
            perController.forEach { (c, cb) -> runCatching { c.unregisterCallback(cb) } }
            perController.clear()
            perCompat.forEach { (c, cb) -> runCatching { c.unregisterCallback(cb) } }
            perCompat.clear()
            compatByToken.keys.retainAll(controllers.map { it.sessionToken }.toSet())
            for (c in controllers) {
                val cb = object : MediaController.Callback() {
                    override fun onPlaybackStateChanged(state: PlaybackState?) = onChanged()
                    override fun onMetadataChanged(metadata: MediaMetadata?) = onChanged()
                    override fun onQueueChanged(queue: MutableList<MediaSession.QueueItem>?) = onChanged()
                    override fun onSessionDestroyed() = onChanged()
                }
                runCatching { c.registerCallback(cb, handler) }
                perController.add(c to cb)
                // shuffle/repeat changes and the async extra-binder attach only surface on the compat callback
                val compat = compatFor(c) ?: continue
                val ccb = object : MediaControllerCompat.Callback() {
                    override fun onShuffleModeChanged(shuffleMode: Int) = onChanged()
                    override fun onRepeatModeChanged(repeatMode: Int) = onChanged()
                    override fun onSessionReady() = onChanged()
                }
                runCatching { compat.registerCallback(ccb, handler) }
                perCompat.add(compat to ccb)
            }
        }

        val setListener = MediaSessionManager.OnActiveSessionsChangedListener { controllers ->
            attachPerController(controllers ?: emptyList())
            onChanged()
        }
        val registered = runCatching {
            manager.addOnActiveSessionsChangedListener(setListener, notificationListener, handler)
        }.isSuccess
        if (registered) {
            attachPerController(runCatching { manager.getActiveSessions(notificationListener) }.getOrDefault(emptyList()))
        }
        return object : MediaSessionListenHandle {
            override fun stop() {
                if (registered) runCatching { manager.removeOnActiveSessionsChangedListener(setListener) }
                synchronized(lock) {
                    perController.forEach { (c, cb) -> runCatching { c.unregisterCallback(cb) } }
                    perController.clear()
                    perCompat.forEach { (c, cb) -> runCatching { c.unregisterCallback(cb) } }
                    perCompat.clear()
                }
            }
        }
    }
}

internal class AndroidSystemMediaSession(
    private val context: Context,
    private val controller: MediaController,
    private val compat: MediaControllerCompat?,
) : SystemMediaSession {
    override val packageName: String get() = controller.packageName

    override fun snapshot(): SystemMediaSnapshot? {
        val metadata = controller.metadata
        val playback = controller.playbackState
        val title = metadata?.getText(MediaMetadata.METADATA_KEY_TITLE)?.toString()?.takeIf { it.isNotEmpty() }
        val artist = (
            metadata?.getText(MediaMetadata.METADATA_KEY_ARTIST)
                ?: metadata?.getText(MediaMetadata.METADATA_KEY_ALBUM_ARTIST)
            )?.toString()?.takeIf { it.isNotEmpty() }
        val album = metadata?.getText(MediaMetadata.METADATA_KEY_ALBUM)?.toString()?.takeIf { it.isNotEmpty() }
        if (title == null && artist == null && album == null) return null
        val durationMs = metadata?.getLong(MediaMetadata.METADATA_KEY_DURATION)?.takeIf { it > 0L }
        val actions = playback?.actions ?: 0L
        val activeId = playback?.activeQueueItemId?.takeIf { it != MediaSession.QueueItem.UNKNOWN_ID.toLong() }
        val playing = playback?.state == PlaybackState.STATE_PLAYING || playback?.state == PlaybackState.STATE_BUFFERING
        val positionAgeMs = playback?.lastPositionUpdateTime?.takeIf { playing && it > 0L }
            ?.let { (SystemClock.elapsedRealtime() - it).coerceAtLeast(0L) }
        val (liked, likeSupported) = readRating(metadata, actions)
        return SystemMediaSnapshot(
            title = title,
            artist = artist,
            album = album,
            durationMs = durationMs,
            positionMs = playback?.position ?: 0L,
            playing = playing,
            canSeek = (actions and PlaybackState.ACTION_SEEK_TO) != 0L,
            artToken = metadata?.let { nowPlayingArtToken(it) },
            queue = readQueue(),
            activeQueueId = activeId,
            shuffle = readShuffle(),
            repeat = readRepeat(),
            speed = playback?.playbackSpeed?.takeIf { playing && it > 0f },
            positionAgeMs = positionAgeMs,
            liked = liked,
            likeSupported = likeSupported,
            queueTitle = runCatching { controller.queueTitle }.getOrNull()?.toString()?.takeIf { it.isNotEmpty() },
        )
    }

    private fun readShuffle(): Boolean? = when (runCatching { compat?.shuffleMode }.getOrNull()) {
        PlaybackStateCompat.SHUFFLE_MODE_NONE -> false
        PlaybackStateCompat.SHUFFLE_MODE_ALL, PlaybackStateCompat.SHUFFLE_MODE_GROUP -> true
        else -> null
    }

    private fun readRepeat(): RepeatMode? = when (runCatching { compat?.repeatMode }.getOrNull()) {
        PlaybackStateCompat.REPEAT_MODE_NONE -> RepeatMode.Off
        PlaybackStateCompat.REPEAT_MODE_ONE -> RepeatMode.One
        PlaybackStateCompat.REPEAT_MODE_ALL, PlaybackStateCompat.REPEAT_MODE_GROUP -> RepeatMode.All
        else -> null
    }

    private fun readRating(metadata: MediaMetadata?, actions: Long): Pair<Boolean?, Boolean> {
        val type = runCatching { controller.ratingType }.getOrDefault(Rating.RATING_NONE)
        if (type != Rating.RATING_HEART && type != Rating.RATING_THUMB_UP_DOWN) return null to false
        val canSet = (actions and PlaybackStateCompat.ACTION_SET_RATING) != 0L
        val rating = metadata?.getRating(MediaMetadata.METADATA_KEY_USER_RATING)
        val liked = when {
            rating == null || !rating.isRated -> false
            type == Rating.RATING_HEART -> rating.hasHeart()
            else -> rating.isThumbUp
        }
        return liked to canSet
    }

    private fun readQueue(): List<SystemMediaQueueEntry> =
        runCatching { controller.queue }.getOrNull().orEmpty().map { item ->
            val d = item.description
            SystemMediaQueueEntry(
                queueId = item.queueId,
                title = d.title?.toString()?.takeIf { it.isNotEmpty() },
                subtitle = d.subtitle?.toString()?.takeIf { it.isNotEmpty() },
                artToken = queueArtToken(item),
            )
        }

    override suspend fun art(token: String): SystemMediaArt? {
        controller.metadata?.let { m ->
            if (nowPlayingArtToken(m) == token) {
                nowPlayingArtUri(m)?.let { return encodeUri(it) }
                nowPlayingArtBitmap(m)?.let { return encodeBitmap(it) }
            }
        }
        val item = runCatching { controller.queue }.getOrNull().orEmpty().firstOrNull { queueArtToken(it) == token }
            ?: return null
        item.description.iconUri?.let { return encodeUri(it) }
        item.description.iconBitmap?.let { return encodeBitmap(it) }
        return null
    }

    private fun nowPlayingArtToken(m: MediaMetadata): String? {
        nowPlayingArtUri(m)?.let { return "u${digest(it.toString())}" }
        val bitmap = nowPlayingArtBitmap(m) ?: return null
        val title = m.getText(MediaMetadata.METADATA_KEY_TITLE) ?: ""
        val album = m.getText(MediaMetadata.METADATA_KEY_ALBUM) ?: ""
        val artist = m.getText(MediaMetadata.METADATA_KEY_ARTIST) ?: ""
        return "b${digest("$title|$album|$artist|${bitmap.width}x${bitmap.height}")}"
    }

    private fun queueArtToken(item: MediaSession.QueueItem): String? {
        item.description.iconUri?.let { return "u${digest(it.toString())}" }
        val bitmap = item.description.iconBitmap ?: return null
        return "b${digest("${item.queueId}|${item.description.title ?: ""}|${bitmap.width}x${bitmap.height}")}"
    }

    private fun nowPlayingArtBitmap(m: MediaMetadata): Bitmap? =
        m.getBitmap(MediaMetadata.METADATA_KEY_ART)
            ?: m.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART)
            ?: m.getBitmap(MediaMetadata.METADATA_KEY_DISPLAY_ICON)

    private fun nowPlayingArtUri(m: MediaMetadata): Uri? =
        listOf(
            MediaMetadata.METADATA_KEY_ART_URI,
            MediaMetadata.METADATA_KEY_ALBUM_ART_URI,
            MediaMetadata.METADATA_KEY_DISPLAY_ICON_URI,
        ).firstNotNullOfOrNull { key ->
            m.getString(key)?.takeIf { it.isNotEmpty() }?.let(Uri::parse)?.takeIf { isLocalUri(it) }
        }

    private fun isLocalUri(uri: Uri): Boolean =
        uri.scheme in setOf("content", "file", "android.resource")

    private fun encodeUri(uri: Uri): SystemMediaArt? = runCatching {
        val bitmap = context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it) }
            ?: return null
        encodeBitmap(bitmap)
    }.getOrNull()

    private fun encodeBitmap(bitmap: Bitmap): SystemMediaArt? = runCatching {
        val edge = maxOf(bitmap.width, bitmap.height)
        val scaled = if (edge <= MAX_ART_EDGE) bitmap else {
            val scale = MAX_ART_EDGE.toFloat() / edge
            Bitmap.createScaledBitmap(
                bitmap,
                (bitmap.width * scale).toInt().coerceAtLeast(1),
                (bitmap.height * scale).toInt().coerceAtLeast(1),
                true,
            )
        }
        val out = ByteArrayOutputStream()
        if (!scaled.compress(Bitmap.CompressFormat.JPEG, ART_JPEG_QUALITY, out)) return null
        SystemMediaArt(bytes = out.toByteArray(), mime = "image/jpeg")
    }.getOrNull()

    private fun digest(value: String): String =
        MessageDigest.getInstance("MD5").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }

    override fun play() { runCatching { controller.transportControls.play() } }
    override fun pause() { runCatching { controller.transportControls.pause() } }
    override fun skipNext() { runCatching { controller.transportControls.skipToNext() } }
    override fun skipPrev() { runCatching { controller.transportControls.skipToPrevious() } }
    override fun seekTo(positionMs: Long) { runCatching { controller.transportControls.seekTo(positionMs) } }
    override fun skipToQueueItem(queueId: Long) { runCatching { controller.transportControls.skipToQueueItem(queueId) } }

    override fun setShuffle(on: Boolean) {
        runCatching {
            compat?.transportControls?.setShuffleMode(
                if (on) PlaybackStateCompat.SHUFFLE_MODE_ALL else PlaybackStateCompat.SHUFFLE_MODE_NONE,
            )
        }
    }

    override fun setRepeat(mode: RepeatMode) {
        val compatMode = when (mode) {
            RepeatMode.Off -> PlaybackStateCompat.REPEAT_MODE_NONE
            RepeatMode.One -> PlaybackStateCompat.REPEAT_MODE_ONE
            RepeatMode.All -> PlaybackStateCompat.REPEAT_MODE_ALL
        }
        runCatching { compat?.transportControls?.setRepeatMode(compatMode) }
    }

    override fun setSpeed(speed: Float) {
        runCatching { compat?.transportControls?.setPlaybackSpeed(speed) }
    }

    override fun setLiked(liked: Boolean) {
        val rating = when (runCatching { controller.ratingType }.getOrDefault(Rating.RATING_NONE)) {
            Rating.RATING_HEART -> Rating.newHeartRating(liked)
            Rating.RATING_THUMB_UP_DOWN -> Rating.newThumbRating(liked)
            else -> return
        }
        runCatching { controller.transportControls.setRating(rating) }
    }

    private companion object {
        const val MAX_ART_EDGE = 512
        const val ART_JPEG_QUALITY = 60
    }
}
