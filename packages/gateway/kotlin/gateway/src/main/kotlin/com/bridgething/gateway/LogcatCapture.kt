package com.bridgething.gateway

import java.io.BufferedReader
import java.io.InputStreamReader

/**
 * Streams this process's logcat output into [LogStore].
 *
 * Reading logcat from an unprivileged app yields only entries logged by the
 * app's own UID - the platform does that filtering for us, so we get every
 * framework, React Native, Nitro and native-library line emitted under our
 * process without needing READ_LOGS and without capturing the whole device.
 * Conversely, lines the system logs about us from *other* processes (bluetooth
 * stack, system_server) are not visible here.
 *
 * `--pid` restricts the dump to the live process, which matters on startup:
 * logcat replays the existing ring buffer before following, so we pick up
 * everything this launch logged before capture started while skipping the
 * previous launch's lines (already persisted under their own launch dir).
 */
public object LogcatCapture {
    private const val TAG = "bridgething-logs"
    private const val RESTART_DELAY_MS = 2_000L

    /**
     * threadtime layout: `MM-DD HH:MM:SS.mmm  PID  TID <level> <tag>: <message>`.
     * Captured groups are level, tag and message; the timestamp is dropped
     * because the UI stamps its own arrival time.
     */
    private val LINE = Regex("""^\d{2}-\d{2} [\d:.]+\s+\d+\s+\d+\s+([VDIWEF])\s+(.*?):\s?(.*)$""")

    @Volatile private var thread: Thread? = null

    /** Idempotent. Starts the reader thread; it supervises and restarts the child process. */
    @Synchronized
    public fun start() {
        if (thread != null) return
        thread = Thread(::run, "bridgething-logcat").apply {
            isDaemon = true
            priority = Thread.MIN_PRIORITY
            start()
        }
    }

    /** Parses a threadtime line and forwards it to the live stream. Unparseable lines go through as debug. */
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
                        // only parse when a stream is actually open; see LocalLogRelay.hasSink
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
            // logcat exited (killed, buffer reset, OOM); back off and reattach
            LogStore.write("<<logcat capture reattaching>>")
            try {
                Thread.sleep(RESTART_DELAY_MS)
            } catch (_: InterruptedException) {
                return
            }
        }
    }
}
