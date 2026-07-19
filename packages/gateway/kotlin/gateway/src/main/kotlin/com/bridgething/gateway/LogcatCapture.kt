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
