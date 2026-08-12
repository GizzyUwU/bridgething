package com.bridgething

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import com.bridgething.companion.CompanionLogs
import java.io.File

public object LogExport {
    private const val AUTHORITY_SUFFIX = ".fileprovider"

    public fun writeBundle(context: Context, archiveId: String? = null): File {
        val dir = File(context.applicationContext.cacheDir, "exports")
        dir.mkdirs()
        dir.listFiles { f: File -> f.name.startsWith("bridgething-logs-") }?.forEach { it.delete() }
        val stamp = android.text.format.DateFormat.format("yyyyMMdd-HHmmss", java.util.Date())
        val suffix = if (archiveId == null) "" else "-$archiveId"
        val store = CompanionLogs.store ?: error("the log store is not installed")
        return File(store.exportTo(File(dir, "bridgething-logs$suffix-$stamp.txt").path, archiveId))
    }

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
        chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return runCatching { app.startActivity(chooser) }.isSuccess
    }
}
