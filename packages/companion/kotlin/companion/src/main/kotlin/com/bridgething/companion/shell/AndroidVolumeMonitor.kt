package com.bridgething.companion.shell

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager
import android.os.Build
import uniffi.bridgething_companion.VolumeInbox
import uniffi.bridgething_companion.VolumeLevel
import uniffi.bridgething_companion.VolumeMonitor

public class AndroidVolumeMonitor(
    context: Context,
) : VolumeMonitor {
    private val appContext = context.applicationContext
    private val audio = appContext.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    @Volatile
    private var receiver: BroadcastReceiver? = null

    @Volatile
    private var heldInbox: VolumeInbox? = null

    override fun start(inbox: VolumeInbox) {
        stop()
        heldInbox = inbox
        val recv = object : BroadcastReceiver() {
            override fun onReceive(c: Context?, intent: Intent?) {
                if (intent?.action != VOLUME_CHANGED_ACTION) return
                val streamType = intent.getIntExtra(EXTRA_VOLUME_STREAM_TYPE, -1)
                if (streamType != AudioManager.STREAM_MUSIC) return
                val level = readSnapshot()
                runCatching { inbox.onChanged(level.level, level.muted) }
            }
        }
        receiver = recv
        val filter = IntentFilter(VOLUME_CHANGED_ACTION)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            appContext.registerReceiver(recv, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            appContext.registerReceiver(recv, filter)
        }
        val level = readSnapshot()
        inbox.onChanged(level.level, level.muted)
    }

    override fun stop() {
        receiver?.let { runCatching { appContext.unregisterReceiver(it) } }
        receiver = null
        val previous = heldInbox
        heldInbox = null
        previous?.close()
    }

    override fun snapshot(): VolumeLevel = readSnapshot()

    private fun readSnapshot(): VolumeLevel {
        val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(1)
        val raw = audio.getStreamVolume(AudioManager.STREAM_MUSIC).coerceAtLeast(0)
        val muted = audio.isStreamMute(AudioManager.STREAM_MUSIC) || raw == 0
        return VolumeLevel(level = raw.toFloat() / max.toFloat(), muted = muted)
    }

    private companion object {
        const val VOLUME_CHANGED_ACTION = "android.media.VOLUME_CHANGED_ACTION"
        const val EXTRA_VOLUME_STREAM_TYPE = "android.media.EXTRA_VOLUME_STREAM_TYPE"
    }
}
