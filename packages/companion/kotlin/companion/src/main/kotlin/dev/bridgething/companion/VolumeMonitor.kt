package dev.bridgething.companion

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager

/**
 * Host audio output volume + mute snapshot via Android's [AudioManager].
 * `STREAM_MUSIC` is the media stream the daemon cares about;
 * `muted == true` follows [AudioManager.isStreamMute].
 *
 * Caller passes an app [Context] at construction. Volume change events
 * are observed via the system `android.media.VOLUME_CHANGED_ACTION`
 * sticky broadcast; this is undocumented but far cheaper than a
 * ContentObserver on `Settings.System` or polling. [start] emits one
 * snapshot immediately so the companion can announce the current level
 * and claim authority on connect.
 */
public class VolumeMonitor(
    private val context: Context,
) : VolumeSource {
    private val audio = context.applicationContext.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var receiver: BroadcastReceiver? = null

    public override fun start(callback: VolumeSource.Callback) {
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
        // RECEIVER_NOT_EXPORTED on Tiramisu+; older releases use the no-flags overload.
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            ctx.registerReceiver(recv, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            ctx.registerReceiver(recv, filter)
        }
        // prime the callback so the companion can claim authority and announce volume on connect.
        val (level, muted) = readSnapshot()
        callback.onVolumeChanged(level, muted)
    }

    public override fun stop() {
        receiver?.let { context.applicationContext.unregisterReceiver(it) }
        receiver = null
    }

    public override fun snapshot(): Pair<Float, Boolean> = readSnapshot()

    private fun readSnapshot(): Pair<Float, Boolean> {
        val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(1)
        val raw = audio.getStreamVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(0)
        val muted = audio.isStreamMute(AudioManager.STREAM_MUSIC) || raw == 0
        val level = raw.toFloat() / max.toFloat()
        return level to muted
    }

    private companion object {
        // AudioManager.VOLUME_CHANGED_ACTION is @hide; the string is stable and required
        // for receiving system volume change broadcasts.
        const val VOLUME_CHANGED_ACTION = "android.media.VOLUME_CHANGED_ACTION"
        const val EXTRA_VOLUME_STREAM_TYPE = "android.media.EXTRA_VOLUME_STREAM_TYPE"
    }
}
