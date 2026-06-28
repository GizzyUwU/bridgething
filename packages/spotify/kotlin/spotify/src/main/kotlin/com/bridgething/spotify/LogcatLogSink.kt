package com.bridgething.spotify

import com.bridgething.gateway.LocalLogRelay
import uniffi.spotify.LogSink

internal class LogcatLogSink : LogSink {
    override fun log(level: String, target: String, message: String) {
        val line = "[$target] $message"
        when (level) {
            "ERROR" -> android.util.Log.e(TAG, line)
            "WARN" -> android.util.Log.w(TAG, line)
            "INFO" -> android.util.Log.i(TAG, line)
            else -> android.util.Log.d(TAG, line)
        }
        LocalLogRelay.push(level, target, message)
    }

    private companion object {
        const val TAG = "spotify"
    }
}
