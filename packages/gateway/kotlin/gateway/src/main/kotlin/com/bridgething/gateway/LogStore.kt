package com.bridgething.gateway

import android.content.Context
import java.io.File
import java.io.IOException
import java.io.Writer
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit

/**
 * Persistent on-disk log storage, laid out as two nested rotating buffers.
 *
 * Outer ring: one directory per app launch, newest [LAUNCH_LIMIT] kept, older
 * pruned at [install] time. Inner ring: within a launch, lines append to a
 * numbered segment file; once a segment reaches [SEGMENT_BYTES] a new one opens
 * and any segment older than the newest [SEGMENTS_PER_LAUNCH] is deleted. So a
 * launch costs at most `SEGMENT_BYTES * SEGMENTS_PER_LAUNCH` on disk and always
 * retains at least the most recent [SEGMENT_BYTES] of output.
 *
 * The segment ring is why the size cap does not simply stop logging when hit:
 * the interesting lines are usually the last ones written, not the first.
 *
 * Writes are handed to a dedicated thread over a bounded queue and never block
 * the caller - log lines arrive on arbitrary threads (the Rust tracing sink, BT
 * callbacks, the logcat reader) and none of them may pay for disk IO.
 */
public object LogStore {
    private const val LAUNCH_LIMIT = 3
    private const val SEGMENTS_PER_LAUNCH = 2
    private const val SEGMENT_BYTES = 512L * 1024
    private const val QUEUE_CAPACITY = 4096

    private val queue = ArrayBlockingQueue<String>(QUEUE_CAPACITY)

    @Volatile private var root: File? = null
    @Volatile private var launchDir: File? = null
    @Volatile private var writer: Thread? = null

    /** Lines dropped because the queue was full; surfaced in the export header. */
    @Volatile private var dropped: Long = 0

    /**
     * Prepares storage and starts the writer thread. Idempotent - a second call
     * is a no-op, so it is safe to call from both Application.onCreate and any
     * later lazy entry point.
     */
    @Synchronized
    public fun install(context: Context) {
        if (writer != null) return

        val dir = File(context.applicationContext.filesDir, "logs")
        dir.mkdirs()
        root = dir

        // launch dirs sort lexicographically because epoch millis is fixed-width
        val launch = File(dir, System.currentTimeMillis().toString())
        launch.mkdirs()
        launchDir = launch
        pruneLaunches(dir)

        writer = Thread(::runWriter, "bridgething-logstore").apply {
            isDaemon = true
            priority = Thread.MIN_PRIORITY
            start()
        }
    }

    /** Queues a line for persistence. Drops rather than blocks when the writer falls behind. */
    public fun write(line: String) {
        if (writer == null) return
        if (!queue.offer(line)) dropped += 1
    }

    // ---- export ----------------------------------------------------------

    /**
     * Concatenates every retained launch, oldest first, into [target].
     * Returns [target] so callers can chain into a share intent.
     */
    public fun exportTo(target: File): File {
        flush()
        target.parentFile?.mkdirs()
        target.bufferedWriter().use { out ->
            out.write("bridgething log export\n")
            out.write("generated: ${java.util.Date()}\n")
            val dirs = launchDirs()
            out.write("launches: ${dirs.size}\n")
            if (dropped > 0) out.write("dropped lines (writer backpressure): $dropped\n")
            out.write("\n")
            for (dir in dirs) {
                val stamp = dir.name.toLongOrNull()?.let { java.util.Date(it) }?.toString() ?: dir.name
                val current = if (dir == launchDir) " (current)" else ""
                out.write("===== launch $stamp$current =====\n")
                for (segment in segments(dir)) {
                    runCatching { segment.forEachLine { out.write(it); out.write("\n") } }
                        .onFailure { out.write("<<unreadable segment ${segment.name}: ${it.message}>>\n") }
                }
                out.write("\n")
            }
        }
        return target
    }

    /** Total bytes currently retained across all launches. */
    public fun retainedBytes(): Long = launchDirs().sumOf { dir -> segments(dir).sumOf { it.length() } }

    /** Drops every retained launch except the live one, which is truncated. */
    @Synchronized
    public fun clear() {
        flush()
        for (dir in launchDirs()) {
            if (dir == launchDir) segments(dir).forEach { it.delete() } else dir.deleteRecursively()
        }
        dropped = 0
    }

    /** Blocks briefly until the writer has drained the queue. Best effort. */
    public fun flush() {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2)
        while (queue.isNotEmpty() && System.nanoTime() < deadline) Thread.sleep(10)
        // the queue empties just before the writer flushes its buffer; give it
        // that window so an export does not clip the last few lines
        Thread.sleep(50)
    }

    // ---- writer thread ---------------------------------------------------

    private fun runWriter() {
        var sink: Sink? = null
        try {
            while (true) {
                val first = queue.take()
                val active = sink ?: openSink().also { sink = it }
                active.write(first)
                // drain whatever else piled up before touching the disk again
                while (true) {
                    val next = queue.poll() ?: break
                    active.write(next)
                }
                active.flush()
                if (active.bytes >= SEGMENT_BYTES) {
                    active.close()
                    sink = null
                }
            }
        } catch (_: InterruptedException) {
            // process teardown; nothing useful left to do
        } catch (e: IOException) {
            android.util.Log.w(TAG, "log writer stopped: ${e.message}")
        } finally {
            runCatching { sink?.close() }
        }
    }

    /** Opens the next segment in the live launch dir and retires anything beyond the ring. */
    private fun openSink(): Sink {
        val dir = launchDir ?: throw IOException("log store not installed")
        val existing = segments(dir)
        // resume the newest segment when it still has room, else start a fresh one
        val newest = existing.lastOrNull()
        val target = if (newest != null && newest.length() < SEGMENT_BYTES) {
            newest
        } else {
            val nextIndex = (newest?.let { segmentIndex(it) } ?: -1) + 1
            File(dir, "%04d.log".format(nextIndex))
        }
        pruneSegments(dir, keepAlso = target)
        return Sink(target)
    }

    private class Sink(file: File) {
        var bytes: Long = file.length()
            private set

        private val out: Writer = java.io.OutputStreamWriter(
            java.io.FileOutputStream(file, true),
            Charsets.UTF_8,
        ).buffered()

        fun write(line: String) {
            out.write(line)
            out.write("\n")
            // +1 for the newline; close enough for a rotation trigger without re-stat'ing
            bytes += line.length.toLong() + 1
        }

        fun flush() = out.flush()

        fun close() {
            runCatching { out.flush() }
            runCatching { out.close() }
        }
    }

    // ---- layout helpers --------------------------------------------------

    private fun launchDirs(): List<File> =
        root?.listFiles { f: File -> f.isDirectory }?.sortedBy { it.name } ?: emptyList()

    private fun segments(dir: File): List<File> =
        dir.listFiles { f: File -> f.isFile && f.name.endsWith(".log") }
            ?.sortedBy { segmentIndex(it) } ?: emptyList()

    private fun segmentIndex(file: File): Int = file.name.removeSuffix(".log").toIntOrNull() ?: -1

    private fun pruneLaunches(dir: File) {
        val dirs = dir.listFiles { f: File -> f.isDirectory }?.sortedBy { it.name } ?: return
        val excess = dirs.size - LAUNCH_LIMIT
        if (excess <= 0) return
        for (i in 0 until excess) dirs[i].deleteRecursively()
    }

    private fun pruneSegments(dir: File, keepAlso: File) {
        val all = (segments(dir) + keepAlso).distinctBy { it.name }.sortedBy { segmentIndex(it) }
        val excess = all.size - SEGMENTS_PER_LAUNCH
        if (excess <= 0) return
        for (i in 0 until excess) if (all[i].name != keepAlso.name) all[i].delete()
    }

    private const val TAG = "bridgething-logs"
}
