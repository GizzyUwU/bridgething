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
 * A segment that takes an error or fatal line is pinned: a sibling `.keep`
 * marker exempts it from both rings, so the launch that went wrong outlives
 * the healthy ones around it and survives until [clear]. [PINNED_BYTES_LIMIT]
 * is the only thing that reclaims pinned segments without asking.
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

    /**
     * Backstop on pinned segments. They are exempt from both rings and only
     * [clear] removes them, so a process stuck in an error loop would otherwise
     * pin every segment it writes and fill the device. Past this the oldest
     * pinned launch is dropped anyway.
     */
    private const val PINNED_BYTES_LIMIT = 32L * 1024 * 1024

    /** Sibling marker file that exempts a segment from rotation. */
    private const val PIN_SUFFIX = ".keep"

    /**
     * logcat threadtime line: `MM-DD HH:MM:SS.mmm  PID  TID <level> TAG: msg`.
     * Only the E (error) and F (fatal) levels pin a segment.
     */
    private val ERROR_LINE =
        Regex("""^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\s+\d+\s+\d+\s+[EF]\s""")

    private val queue = ArrayBlockingQueue<String>(QUEUE_CAPACITY)

    @Volatile private var root: File? = null
    @Volatile private var launchDir: File? = null
    @Volatile private var writer: Thread? = null

    /**
     * Bumped whenever segments are deleted out from under the writer. It keeps
     * its file handle open across writes, so without this it would go on
     * appending to an unlinked inode until the next rotation.
     */
    @Volatile private var sinkGeneration = 0

    /** Launch directories are named for their start time, so ids are always digits. */
    private val ID = Regex("""^\d+$""")

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

    /** One retained launch, as surfaced to the UI. */
    public data class LogArchive(
        val id: String,
        val startedAtMs: Long,
        val bytes: Long,
        val pinned: Boolean,
        val current: Boolean,
    )

    /** Retained launches, newest first. */
    public fun archives(): List<LogArchive> =
        launchDirs()
            .map { dir ->
                LogArchive(
                    id = dir.name,
                    startedAtMs = dir.name.toLongOrNull() ?: 0L,
                    bytes = segments(dir).sumOf { it.length() },
                    pinned = pinnedSegments(dir).isNotEmpty(),
                    current = dir == launchDir,
                )
            }
            .sortedByDescending { it.startedAtMs }

    /** Drops a single launch. Truncates in place when it is the live one. */
    @Synchronized
    public fun delete(id: String) {
        val base = root ?: return
        if (!ID.matches(id)) return
        val dir = File(base, id)
        if (!dir.isDirectory) return
        flush()
        if (dir == launchDir) {
            segments(dir).forEach { pinMarker(it).delete(); it.delete() }
            sinkGeneration += 1
        } else {
            dir.deleteRecursively()
        }
    }

    /**
     * Concatenates retained launches, oldest first, into [target]. Passing an
     * [id] narrows the bundle to that one launch.
     * Returns [target] so callers can chain into a share intent.
     */
    public fun exportTo(target: File, id: String? = null): File {
        flush()
        target.parentFile?.mkdirs()
        target.bufferedWriter().use { out ->
            out.write("bridgething log export\n")
            out.write("generated: ${java.util.Date()}\n")
            val dirs = if (id == null) launchDirs() else launchDirs().filter { it.name == id }
            out.write("launches: ${dirs.size}\n")
            if (dropped > 0) out.write("dropped lines (writer backpressure): $dropped\n")
            out.write("\n")
            for (dir in dirs) {
                val stamp = dir.name.toLongOrNull()?.let { java.util.Date(it) }?.toString() ?: dir.name
                val current = if (dir == launchDir) " (current)" else ""
                val pinned = if (pinnedSegments(dir).isNotEmpty()) " [pinned: contains errors]" else ""
                out.write("===== launch $stamp$current$pinned =====\n")
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

    /**
     * Drops every retained launch except the live one, which is truncated.
     * This is the only thing that removes pinned error segments.
     */
    @Synchronized
    public fun clear() {
        flush()
        for (dir in launchDirs()) {
            if (dir == launchDir) {
                segments(dir).forEach { pinMarker(it).delete(); it.delete() }
            } else {
                dir.deleteRecursively()
            }
        }
        sinkGeneration += 1
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
        var generation = sinkGeneration
        try {
            while (true) {
                val first = queue.take()
                if (generation != sinkGeneration) {
                    runCatching { sink?.close() }
                    sink = null
                    generation = sinkGeneration
                }
                val active = sink ?: openSink().also { sink = it }
                active.write(first)
                // drain whatever else piled up before touching the disk again
                while (true) {
                    val next = queue.poll() ?: break
                    active.write(next)
                }
                if (active.sawError) active.pin()
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

    private class Sink(private val file: File) {
        var bytes: Long = file.length()
            private set

        /** Set once this segment has taken an error line; cleared by [pin]. */
        var sawError: Boolean = false
            private set

        private var pinned: Boolean = false

        private val out: Writer = java.io.OutputStreamWriter(
            java.io.FileOutputStream(file, true),
            Charsets.UTF_8,
        ).buffered()

        fun write(line: String) {
            out.write(line)
            out.write("\n")
            // +1 for the newline; close enough for a rotation trigger without re-stat'ing
            bytes += line.length.toLong() + 1
            if (!pinned && ERROR_LINE.containsMatchIn(line)) sawError = true
        }

        /** Drops the marker that exempts this segment from rotation. Idempotent. */
        fun pin() {
            sawError = false
            if (pinned) return
            pinned = runCatching { pinMarker(file).createNewFile() || pinMarker(file).exists() }
                .getOrDefault(false)
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

    /** Marker path that pins [segment] against rotation. */
    private fun pinMarker(segment: File): File =
        File(segment.parentFile, segment.name.removeSuffix(".log") + PIN_SUFFIX)

    private fun isPinned(segment: File): Boolean = pinMarker(segment).exists()

    private fun pinnedSegments(dir: File): List<File> = segments(dir).filter { isPinned(it) }

    /**
     * Applies the launch ring to unpinned launches only, then enforces
     * [PINNED_BYTES_LIMIT] over what the pins held back.
     */
    private fun pruneLaunches(dir: File) {
        val dirs = dir.listFiles { f: File -> f.isDirectory }?.sortedBy { it.name } ?: return
        val (pinned, rotating) = dirs.partition { pinnedSegments(it).isNotEmpty() }

        val excess = rotating.size - LAUNCH_LIMIT
        for (i in 0 until excess) rotating[i].deleteRecursively()

        // oldest first, so the cap sheds the least interesting errors
        var total = pinned.sumOf { d -> pinnedSegments(d).sumOf { it.length() } }
        for (d in pinned) {
            if (total <= PINNED_BYTES_LIMIT) break
            total -= pinnedSegments(d).sumOf { it.length() }
            android.util.Log.w(TAG, "pinned log cap hit; dropping error launch ${d.name}")
            d.deleteRecursively()
        }
    }

    /** The segment ring applies only to unpinned segments; pinned ones accumulate beside them. */
    private fun pruneSegments(dir: File, keepAlso: File) {
        val all = (segments(dir) + keepAlso).distinctBy { it.name }.sortedBy { segmentIndex(it) }
        val rotating = all.filterNot { isPinned(it) }
        val excess = rotating.size - SEGMENTS_PER_LAUNCH
        if (excess <= 0) return
        for (i in 0 until excess) if (rotating[i].name != keepAlso.name) rotating[i].delete()
    }

    private const val TAG = "bridgething-logs"
}
