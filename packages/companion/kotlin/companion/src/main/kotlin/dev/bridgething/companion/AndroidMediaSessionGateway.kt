package dev.bridgething.companion

import android.content.ComponentName
import android.content.Context
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Handler
import android.os.HandlerThread
import androidx.core.app.NotificationManagerCompat

/**
 * Real [MediaSessionGateway] over [MediaSessionManager]. The host supplies the ComponentName of its
 * enabled NotificationListenerService - that grant authorizes getActiveSessions plus the active-sessions
 * listener with no extra permission. A dedicated handler thread backs the framework callbacks (they need a
 * Looper and the companion attaches off the main thread).
 */
public class AndroidMediaSessionGateway(
    context: Context,
    private val notificationListener: ComponentName,
) : MediaSessionGateway {
    private val appContext = context.applicationContext
    private val manager = appContext.getSystemService(Context.MEDIA_SESSION_SERVICE) as MediaSessionManager
    private val thread = HandlerThread("bridgething-media-sessions").apply { start() }
    private val handler = Handler(thread.looper)

    override val isAccessGranted: Boolean
        get() = NotificationManagerCompat.getEnabledListenerPackages(appContext).contains(appContext.packageName)

    override fun activeSessions(): List<SystemMediaSession> =
        runCatching { manager.getActiveSessions(notificationListener) }
            .getOrDefault(emptyList())
            .map { AndroidSystemMediaSession(it) }

    override fun listen(onChanged: () -> Unit): MediaSessionListenHandle {
        val lock = Any()
        val perController = mutableListOf<Pair<MediaController, MediaController.Callback>>()

        fun attachPerController(controllers: List<MediaController>) = synchronized(lock) {
            perController.forEach { (c, cb) -> runCatching { c.unregisterCallback(cb) } }
            perController.clear()
            for (c in controllers) {
                val cb = object : MediaController.Callback() {
                    override fun onPlaybackStateChanged(state: PlaybackState?) = onChanged()
                    override fun onMetadataChanged(metadata: MediaMetadata?) = onChanged()
                    override fun onSessionDestroyed() = onChanged()
                }
                runCatching { c.registerCallback(cb, handler) }
                perController.add(c to cb)
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
                }
            }
        }
    }
}

internal class AndroidSystemMediaSession(private val controller: MediaController) : SystemMediaSession {
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
        return SystemMediaSnapshot(
            title = title,
            artist = artist,
            album = album,
            durationMs = durationMs,
            positionMs = playback?.position ?: 0L,
            playing = playback?.state == PlaybackState.STATE_PLAYING,
            canSeek = (actions and PlaybackState.ACTION_SEEK_TO) != 0L,
        )
    }

    override fun play() { runCatching { controller.transportControls.play() } }
    override fun pause() { runCatching { controller.transportControls.pause() } }
    override fun skipNext() { runCatching { controller.transportControls.skipToNext() } }
    override fun skipPrev() { runCatching { controller.transportControls.skipToPrevious() } }
    override fun seekTo(positionMs: Long) { runCatching { controller.transportControls.seekTo(positionMs) } }
}
