package dev.bridgething.companion

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager

/**
 * Host audio output volume + mute snapshot via Android's [AudioManager].
 * Android exposes `STREAM_MUSIC` as the media stream the daemon cares
 * about; `muted == true` follows [AudioManager.isStreamMute].
 *
 * Mirror of Swift `VolumeMonitor`. Android-only: caller passes an app
 * [Context] at construction. Volume change events are observed via the
 * system `android.media.VOLUME_CHANGED_ACTION` sticky broadcast; this is
 * undocumented but widely relied on and far cheaper than the alternatives
 * (ContentObserver on `Settings.System` or polling). When [start] is
 * called we emit one snapshot immediately so the companion can announce
 * the current level + claim authority on connect.
 */
public class VolumeMonitor(
    private val context: Context,
) {
    public fun interface Callback {
        public fun onVolumeChanged(level: Float, muted: Boolean)
    }

    private val audio = context.applicationContext.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var receiver: BroadcastReceiver? = null

    public fun start(callback: Callback) {
        stop()
        val ctx = context.applicationContext
        val recv = object : BroadcastReceiver() {
            override fun onReceive(c: Context?, intent: Intent?) {
                if (intent?.action != VOLUME_CHANGED_ACTION) return
                val streamType = intent.getIntExtra(EXTRA_VOLUME_STREAM_TYPE, -1)
                if (streamType != AudioManager.STREAM_MUSIC) return
                val (level, muted) = readSnapshot()
                callback.onVolumeChanged(level, muted)
            }
        }
        receiver = recv
        val filter = IntentFilter(VOLUME_CHANGED_ACTION)
        // RECEIVER_NOT_EXPORTED on Tiramisu+; on older releases the
        // overload without flags is the right one.
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            ctx.registerReceiver(recv, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            ctx.registerReceiver(recv, filter)
        }
        // Prime the callback with the current value so the companion can
        // claim authority + announce volume on connect.
        val (level, muted) = readSnapshot()
        callback.onVolumeChanged(level, muted)
    }

    public fun stop() {
        receiver?.let { context.applicationContext.unregisterReceiver(it) }
        receiver = null
    }

    public fun snapshot(): Pair<Float, Boolean> = readSnapshot()

    private fun readSnapshot(): Pair<Float, Boolean> {
        val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(1)
        val raw = audio.getStreamVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(0)
        val muted = audio.isStreamMute(AudioManager.STREAM_MUSIC) || raw == 0
        val level = raw.toFloat() / max.toFloat()
        return level to muted
    }

    private companion object {
        // android.media.AudioManager.VOLUME_CHANGED_ACTION is @hide; the
        // string is stable across releases and required for receiving
        // system volume change broadcasts.
        const val VOLUME_CHANGED_ACTION = "android.media.VOLUME_CHANGED_ACTION"
        const val EXTRA_VOLUME_STREAM_TYPE = "android.media.EXTRA_VOLUME_STREAM_TYPE"
    }
}
