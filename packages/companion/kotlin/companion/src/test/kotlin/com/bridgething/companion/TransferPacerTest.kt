package com.bridgething.companion

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class TransferPacerTest {
    private class FakeClock {
        var now: Double = 0.0
        fun advance(seconds: Double) {
            now += seconds
        }
    }

    private fun makePacer(startOffset: Long = 0L): Pair<TransferPacer, FakeClock> {
        val clock = FakeClock()
        return TransferPacer(startOffset) { clock.now } to clock
    }

    @Test
    fun `initial window is one large fragment`() {
        val (pacer, _) = makePacer()
        assertEquals(TransferPacer.LARGE_FRAGMENT_BYTES.toLong(), pacer.windowBytes)
        assertEquals(TransferPacer.LARGE_FRAGMENT_BYTES, pacer.fragmentBytes)
    }

    @Test
    fun `fast acks ride the cap with large fragments`() {
        val (pacer, clock) = makePacer()
        var acked = 0L
        repeat(8) {
            clock.advance(0.1)
            acked += 16 * 1024
            pacer.observe(acked)
        }
        assertEquals(TransferPacer.MAX_WINDOW_BYTES, pacer.windowBytes)
        assertEquals(TransferPacer.LARGE_FRAGMENT_BYTES, pacer.fragmentBytes)
    }

    @Test
    fun `slow acks bound queue to the target delay with small fragments`() {
        val (pacer, clock) = makePacer()
        var acked = 0L
        repeat(10) {
            clock.advance(2.0)
            acked += 16 * 1024
            pacer.observe(acked)
        }
        assertTrue(pacer.windowBytes <= 8 * 1024, "8 KB/s x 0.6s target delay stays under a second of queue")
        assertTrue(pacer.windowBytes >= TransferPacer.MIN_WINDOW_BYTES)
        assertEquals(TransferPacer.SMALL_FRAGMENT_BYTES, pacer.fragmentBytes)
    }

    @Test
    fun `degrading link sheds queue on one long ack gap`() {
        val (pacer, clock) = makePacer()
        var acked = 0L
        repeat(8) {
            clock.advance(0.1)
            acked += 16 * 1024
            pacer.observe(acked)
        }
        assertEquals(TransferPacer.MAX_WINDOW_BYTES, pacer.windowBytes)
        clock.advance(3.0)
        acked += 4 * 1024
        pacer.observe(acked)
        assertTrue(pacer.windowBytes < TransferPacer.MAX_WINDOW_BYTES)
    }

    @Test
    fun `recovery grows the window back`() {
        val (pacer, clock) = makePacer()
        var acked = 0L
        repeat(10) {
            clock.advance(2.0)
            acked += 4 * 1024
            pacer.observe(acked)
        }
        assertEquals(TransferPacer.MIN_WINDOW_BYTES, pacer.windowBytes)
        repeat(20) {
            clock.advance(0.05)
            acked += 16 * 1024
            pacer.observe(acked)
        }
        assertEquals(TransferPacer.MAX_WINDOW_BYTES, pacer.windowBytes)
        assertEquals(TransferPacer.LARGE_FRAGMENT_BYTES, pacer.fragmentBytes)
    }

    @Test
    fun `non-advancing ack is ignored`() {
        val (pacer, clock) = makePacer(startOffset = 8 * 1024)
        clock.advance(5.0)
        pacer.observe(8 * 1024)
        pacer.observe(4 * 1024)
        assertEquals(TransferPacer.LARGE_FRAGMENT_BYTES.toLong(), pacer.windowBytes, "no rate estimate without progress")
    }
}
