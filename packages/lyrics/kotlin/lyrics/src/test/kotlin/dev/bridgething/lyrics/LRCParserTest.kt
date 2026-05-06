package dev.bridgething.lyrics

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class LRCParserTest {
    @Test
    fun `parses single-timestamp lines`() {
        val input = """
            [00:12.50]Line one
            [00:17.00]Line two
            [01:30.50]Line three
        """.trimIndent()
        val lines = LRCParser.parse(input)
        assertEquals(3, lines.size)
        assertEquals(LyricLine(12500, "Line one"), lines[0])
        assertEquals(LyricLine(17000, "Line two"), lines[1])
        assertEquals(LyricLine(90500, "Line three"), lines[2])
    }

    @Test
    fun `expands multiple timestamps on one line`() {
        val input = "[00:12.00][01:30.00]Repeated chorus"
        val lines = LRCParser.parse(input)
        assertEquals(2, lines.size)
        assertEquals(LyricLine(12000, "Repeated chorus"), lines[0])
        assertEquals(LyricLine(90000, "Repeated chorus"), lines[1])
    }

    @Test
    fun `skips lines without timestamps`() {
        val input = """
            [ti: Title]
            [ar: Artist]
            [00:12.50]Line one
        """.trimIndent()
        val lines = LRCParser.parse(input)
        assertEquals(1, lines.size)
        assertEquals("Line one", lines[0].text)
    }

    @Test
    fun `accepts three-digit fractional seconds`() {
        val lines = LRCParser.parse("[00:12.500]Line")
        assertEquals(1, lines.size)
        assertEquals(12500, lines[0].startMs)
    }

    @Test
    fun `emits sorted by timestamp`() {
        val lines = LRCParser.parse("[01:30.00]B\n[00:12.50]A")
        assertEquals("A", lines[0].text)
        assertEquals("B", lines[1].text)
    }
}
