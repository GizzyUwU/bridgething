package com.bridgething.companion

import android.content.Context
import java.io.File
import uniffi.bridgething_companion.LogStore

public object CompanionLogs {
    @Volatile
    private var installed: LogStore? = null

    public val store: LogStore?
        get() = installed

    public fun defaultDir(context: Context): File = File(context.applicationContext.filesDir, "logs")

    @Synchronized
    public fun install(context: Context): LogStore = install(defaultDir(context))

    @Synchronized
    public fun install(dir: File): LogStore {
        installed?.let { return it }
        dir.mkdirs()
        return LogStore.install(dir.path).also { installed = it }
    }
}
