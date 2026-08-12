package com.bridgething.companion.shell

import android.content.Context
import android.content.Intent
import android.view.KeyEvent
import uniffi.bridgething_companion.DeviceWaker
import uniffi.bridgething_companion.WakeReason

public class IntentDeviceWaker(
    context: Context,
) : DeviceWaker {
    private val appContext = context.applicationContext

    override fun wakeDevice(reason: WakeReason) {
        for (action in intArrayOf(KeyEvent.ACTION_DOWN, KeyEvent.ACTION_UP)) {
            val intent = Intent(Intent.ACTION_MEDIA_BUTTON).apply {
                setPackage(SPOTIFY_ANDROID_PACKAGE)
                putExtra(Intent.EXTRA_KEY_EVENT, KeyEvent(action, KeyEvent.KEYCODE_MEDIA_PLAY))
            }
            runCatching { appContext.sendBroadcast(intent) }
        }
    }

    private companion object {
        const val SPOTIFY_ANDROID_PACKAGE = "com.spotify.music"
    }
}
