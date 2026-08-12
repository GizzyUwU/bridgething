package com.bridgething.companion.shell

import android.util.Log
import uniffi.bridgething_companion.LogLevel
import uniffi.bridgething_companion.LogSink

public class LogcatSink(
    private val tag: String = "bridgething",
) : LogSink {
    override fun onLine(level: LogLevel, target: String, message: String) {
        val priority = when (level) {
            LogLevel.TRACE -> Log.VERBOSE
            LogLevel.DEBUG -> Log.DEBUG
            LogLevel.INFO -> Log.INFO
            LogLevel.WARN -> Log.WARN
            LogLevel.ERROR -> Log.ERROR
        }
        Log.println(priority, tag, "$target: $message")
    }
}
