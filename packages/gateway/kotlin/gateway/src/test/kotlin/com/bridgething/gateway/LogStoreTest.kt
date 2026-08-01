package com.bridgething.gateway

import org.junit.jupiter.api.AfterAll
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import java.io.File

class LogStoreTest {
    @BeforeEach
    fun reset() {
        LogStore.clear()
    }

    private fun liveLaunch(): File = File(root, LogStore.archives().first { it.current }.id)

    private fun segments(dir: File): List<File> =
        dir.listFiles { f: File -> f.isFile && f.name.endsWith(".log") }?.sortedBy { it.name } ?: emptyList()

    private fun exportText(): String {
        val target = File(root, "bundle.txt")
        LogStore.exportTo(target)
        return target.readText()
    }

    @Test
    fun `a burst larger than a segment rolls instead of overflowing one file`() {
        val body = "x".repeat(380)
        repeat(6000) { LogStore.write("07-30 12:00:00.000  1  1 I burst: $it $body") }
        LogStore.flush()

        val segments = segments(liveLaunch())
        assertEquals(2, segments.size, "segment ring should hold at $SEGMENTS_PER_LAUNCH")
        for (segment in segments) {
            assertTrue(
                segment.length() <= SEGMENT_BYTES + MAX_LINE_SLOP,
                "${segment.name} is ${segment.length()} bytes, over the ${SEGMENT_BYTES} cap",
            )
        }
    }

    @Test
    fun `a record with no logcat prefix still pins on error`() {
        LogStore.record(LogStore.Level.ERROR, "daemon", "[player] the thing that went wrong")
        LogStore.flush()

        assertTrue(File(liveLaunch(), "0000.keep").exists(), "an error record should pin its segment")
        assertTrue(LogStore.archives().first { it.current }.pinned)
    }

    @Test
    fun `a record below error does not pin`() {
        for (level in listOf(LogStore.Level.VERBOSE, LogStore.Level.DEBUG, LogStore.Level.INFO, LogStore.Level.WARN)) {
            LogStore.record(level, "daemon", "noise")
        }
        LogStore.flush()

        assertTrue(!File(liveLaunch(), "0000.keep").exists())
        assertTrue(!LogStore.archives().first { it.current }.pinned)
    }

    @Test
    fun `a record carries the threadtime prefix the logcat reader delivers`() {
        LogStore.record(LogStore.Level.WARN, "daemon", "[player] stalled")
        LogStore.flush()

        val line = exportText().lineSequence().first { it.contains("stalled") }
        assertTrue(
            Regex("""^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\s+\d+\s+\d+\s+W daemon: \[player] stalled$""").matches(line),
            "unexpected line shape: $line",
        )
    }

    @Test
    fun `a multi-line record becomes one prefixed line each`() {
        LogStore.record(LogStore.Level.INFO, "daemon", "first\nsecond")
        LogStore.flush()

        val text = exportText()
        assertEquals(2, text.split(" I daemon: ").size - 1)
        assertTrue(text.contains(" I daemon: first"))
        assertTrue(text.contains(" I daemon: second"))
    }

    private companion object {
        const val SEGMENT_BYTES = 512L * 1024
        const val SEGMENTS_PER_LAUNCH = 2

        const val MAX_LINE_SLOP = 4096L

        lateinit var root: File

        @BeforeAll
        @JvmStatic
        fun install() {
            root = File(System.getProperty("java.io.tmpdir"), "logstore-${System.nanoTime()}")
            LogStore.install(root)
        }

        @AfterAll
        @JvmStatic
        fun cleanup() {
            root.deleteRecursively()
        }
    }
}
