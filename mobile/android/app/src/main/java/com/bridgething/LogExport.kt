package com.bridgething

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import com.bridgething.gateway.LogStore
import java.io.File

/**
 * Turns the retained [LogStore] launches into a single text bundle and hands it
 * to the OS share sheet.
 *
 * The bundle is written under `cacheDir/exports`, which is the only path the
 * app's FileProvider exposes; sharing outside the app requires a content:// URI
 * with a transient read grant, since a file:// URI to internal storage is both
 * unreadable by the receiving app and a StrictMode violation to emit.
 */
public object LogExport {
    private const val AUTHORITY_SUFFIX = ".fileprovider"

    /**
     * Writes a fresh bundle and returns it. Overwrites nothing - each call is
     * timestamped. Passing an [archiveId] narrows it to that single launch.
     */
    public fun writeBundle(context: Context, archiveId: String? = null): File {
        val dir = File(context.applicationContext.cacheDir, "exports")
        dir.mkdirs()
        // one stale bundle per share is enough; drop older ones so the cache does not creep
        dir.listFiles { f: File -> f.name.startsWith("bridgething-logs-") }?.forEach { it.delete() }
        val stamp = android.text.format.DateFormat.format("yyyyMMdd-HHmmss", java.util.Date())
        val suffix = if (archiveId == null) "" else "-$archiveId"
        return LogStore.exportTo(File(dir, "bridgething-logs$suffix-$stamp.txt"), archiveId)
    }

    /**
     * Opens the system share sheet for a bundle from [writeBundle]. Must run on
     * the main thread. Returns false when no activity is available to host the
     * chooser and the app context cannot start one either.
     */
    public fun share(context: Context, file: File): Boolean {
        val app = context.applicationContext
        val uri = FileProvider.getUriForFile(app, app.packageName + AUTHORITY_SUFFIX, file)

        val send = Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_STREAM, uri)
            putExtra(Intent.EXTRA_SUBJECT, file.name)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        val chooser = Intent.createChooser(send, "Share bridgething logs").apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }

        val activity = BridgethingActivityRegistry.currentActivity
        if (activity != null) {
            activity.startActivity(chooser)
            return true
        }
        // backgrounded: a chooser from app context needs its own task
        chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return runCatching { app.startActivity(chooser) }.isSuccess
    }
}
