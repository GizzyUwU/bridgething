package com.bridgething.gateway

import java.io.BufferedReader
import java.io.InputStreamReader

public object LogcatCapture {
    private const val TAG = "bridgething-logs"
    private const val RESTART_DELAY_MS = 2_000L

    private val LINE = Regex("""^\d{2}-\d{2} [\d:.]+\s+\d+\s+\d+\s+([VDIWEF])\s+(.*?):\s?(.*)$""")

    @Volatile private var thread: Thread? = null

    @Synchronized
    public fun start() {
        if (thread != null) return
        thread = Thread(::run, "bridgething-logcat").apply {
            isDaemon = true
            priority = Thread.MIN_PRIORITY
            start()
        }
    }

    private fun relay(line: String) {
        val m = LINE.find(line)
        if (m == null) {
            LocalLogRelay.push("DEBUG", "logcat", line)
            return
        }
        val (level, tag, message) = m.destructured
        val name = when (level) {
            "E", "F" -> "ERROR"
            "W" -> "WARN"
            "I" -> "INFO"
            else -> "DEBUG"
        }
        LocalLogRelay.push(name, tag.trim(), message)
    }

    private fun run() {
        val pid = android.os.Process.myPid().toString()
        while (true) {
            var process: Process? = null
            try {
                process = ProcessBuilder("logcat", "--pid=$pid", "-v", "threadtime")
                    .redirectErrorStream(true)
                    .start()
                BufferedReader(InputStreamReader(process.inputStream, Charsets.UTF_8)).use { reader ->
                    while (true) {
                        val line = reader.readLine() ?: break
                        LogStore.write(line)
                        if (LocalLogRelay.hasSink()) relay(line)
                    }
                }
            } catch (_: InterruptedException) {
                process?.destroy()
                return
            } catch (e: Exception) {
                LogStore.write("<<logcat capture error: ${e.message}>>")
                android.util.Log.w(TAG, "logcat capture failed", e)
            } finally {
                runCatching { process?.destroy() }
            }
            LogStore.write("<<logcat capture reattaching>>")
            try {
                Thread.sleep(RESTART_DELAY_MS)
            } catch (_: InterruptedException) {
                return
            }
        }
    }
}
