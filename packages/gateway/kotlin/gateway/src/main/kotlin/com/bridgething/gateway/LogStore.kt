package com.bridgething.gateway

import android.content.Context
import java.io.File
import java.io.IOException
import java.io.Writer
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit

public object LogStore {
    private const val LAUNCH_LIMIT = 3
    private const val SEGMENTS_PER_LAUNCH = 2
    private const val SEGMENT_BYTES = 512L * 1024
    private const val QUEUE_CAPACITY = 4096

    private const val PINNED_BYTES_LIMIT = 32L * 1024 * 1024
    private const val PIN_SUFFIX = ".keep"
    private val ERROR_LINE =
        Regex("""^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\s+\d+\s+\d+\s+[EF]\s""")
    private val queue = ArrayBlockingQueue<String>(QUEUE_CAPACITY)
    private val ID = Regex("""^\d+$""")

    @Volatile private var root: File? = null
    @Volatile private var launchDir: File? = null
    @Volatile private var writer: Thread? = null
    @Volatile private var sinkGeneration = 0
    @Volatile private var dropped: Long = 0

    public fun install(context: Context) {
        install(File(context.applicationContext.filesDir, "logs"))
    }

    @Synchronized
    public fun install(dir: File) {
        if (writer != null) return

        dir.mkdirs()
        root = dir

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

    public fun write(line: String) {
        if (writer == null) return
        if (!queue.offer(line)) dropped += 1
    }

    public enum class Level(internal val letter: Char) {
        VERBOSE('V'),
        DEBUG('D'),
        INFO('I'),
        WARN('W'),
        ERROR('E'),
        FATAL('F'),
    }

    public fun record(level: Level, tag: String, message: String) {
        if (writer == null) return
        val prefix = "${stamp()} ${android.os.Process.myPid()} ${android.os.Process.myTid()} ${level.letter} $tag: "
        for (part in message.split('\n')) write(prefix + part)
    }

    private val STAMP = java.text.SimpleDateFormat("MM-dd HH:mm:ss.SSS", java.util.Locale.US)

    private fun stamp(): String = synchronized(STAMP) { STAMP.format(java.util.Date()) }

    // ---- export ----------------------------------------------------------

    public data class LogArchive(
        val id: String,
        val startedAtMs: Long,
        val bytes: Long,
        val pinned: Boolean,
        val current: Boolean,
    )

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

    public fun retainedBytes(): Long = launchDirs().sumOf { dir -> segments(dir).sumOf { it.length() } }

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

    public fun flush() {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2)
        while (queue.isNotEmpty() && System.nanoTime() < deadline) Thread.sleep(10)
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
                var line: String? = first
                while (line != null) {
                    val active = sink ?: openSink().also { sink = it }
                    active.write(line)
                    if (active.bytes >= SEGMENT_BYTES) {
                        if (active.sawError) active.pin()
                        active.close()
                        sink = null
                    }
                    line = queue.poll()
                }
                sink?.let {
                    if (it.sawError) it.pin()
                    it.flush()
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

    private fun openSink(): Sink {
        val dir = launchDir ?: throw IOException("log store not installed")
        val existing = segments(dir)
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
            bytes += line.length.toLong() + 1
            if (!pinned && ERROR_LINE.containsMatchIn(line)) sawError = true
        }

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

    private fun pinMarker(segment: File): File =
        File(segment.parentFile, segment.name.removeSuffix(".log") + PIN_SUFFIX)

    private fun isPinned(segment: File): Boolean = pinMarker(segment).exists()

    private fun pinnedSegments(dir: File): List<File> = segments(dir).filter { isPinned(it) }

    private fun pruneLaunches(dir: File) {
        val dirs = dir.listFiles { f: File -> f.isDirectory }?.sortedBy { it.name } ?: return
        val (pinned, rotating) = dirs.partition { pinnedSegments(it).isNotEmpty() }

        val excess = rotating.size - LAUNCH_LIMIT
        for (i in 0 until excess) rotating[i].deleteRecursively()

        var total = pinned.sumOf { d -> pinnedSegments(d).sumOf { it.length() } }
        for (d in pinned) {
            if (total <= PINNED_BYTES_LIMIT) break
            total -= pinnedSegments(d).sumOf { it.length() }
            android.util.Log.w(TAG, "pinned log cap hit; dropping error launch ${d.name}")
            d.deleteRecursively()
        }
    }

    private fun pruneSegments(dir: File, keepAlso: File) {
        val all = (segments(dir) + keepAlso).distinctBy { it.name }.sortedBy { segmentIndex(it) }
        val rotating = all.filterNot { isPinned(it) }
        val excess = rotating.size - SEGMENTS_PER_LAUNCH
        if (excess <= 0) return
        for (i in 0 until excess) if (rotating[i].name != keepAlso.name) rotating[i].delete()
    }

    private const val TAG = "bridgething-logs"
}
