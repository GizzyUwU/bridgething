package com.bridgething.companion

import java.io.BufferedReader
import java.io.InputStreamReader
import uniffi.bridgething_companion.LogInbox
import uniffi.bridgething_companion.LogLevel

public object LogcatCapture {
    private const val TAG = "bridgething-logs"
    private const val RESTART_DELAY_MS = 2_000L

    @Volatile private var thread: Thread? = null

    @Volatile private var inbox: LogInbox? = null

    @Synchronized
    public fun setInbox(next: LogInbox?) {
        val previous = inbox
        inbox = next
        if (previous !== next) previous?.close()
    }

    @Synchronized
    public fun start() {
        if (thread != null) return
        thread = Thread(::run, "bridgething-logcat").apply {
            isDaemon = true
            priority = Thread.MIN_PRIORITY
            start()
        }
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
                        CompanionLogs.store?.write(line)
                        relay(line)
                    }
                }
            } catch (_: InterruptedException) {
                process?.destroy()
                return
            } catch (e: Exception) {
                CompanionLogs.store?.write("<<logcat capture error: ${e.message}>>")
                android.util.Log.w(TAG, "logcat capture failed", e)
            } finally {
                runCatching { process?.destroy() }
            }
            CompanionLogs.store?.write("<<logcat capture reattaching>>")
            try {
                Thread.sleep(RESTART_DELAY_MS)
            } catch (_: InterruptedException) {
                return
            }
        }
    }

    private fun relay(line: String) {
        val held = inbox ?: return
        val parsed = parse(line)
        runCatching { held.push(parsed.level, parsed.tag, parsed.message) }
    }

    private data class Parsed(val level: LogLevel, val tag: String, val message: String)

    private val THREADTIME = Regex("""^\d{2}-\d{2}\s+\S+\s+\d+\s+\d+\s+([VDIWEF])\s+(.*?)\s*: (.*)$""")

    private fun parse(line: String): Parsed {
        val match = THREADTIME.matchEntire(line) ?: return Parsed(LogLevel.DEBUG, "logcat", line)
        val level = when (match.groupValues[1]) {
            "V" -> LogLevel.TRACE
            "D" -> LogLevel.DEBUG
            "I" -> LogLevel.INFO
            "W" -> LogLevel.WARN
            else -> LogLevel.ERROR
        }
        return Parsed(level, match.groupValues[2], match.groupValues[3])
    }
}
