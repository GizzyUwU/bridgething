package com.bridgething.companion

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
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

    private data class Run(val throughput: Double, val window: Long)

    private fun simulate(linkBytesPerSec: Double, rtt: Double, seconds: Double): Run {
        val (pacer, clock) = makePacer()
        var acked = 0L
        var elapsed = 0.0
        while (elapsed < seconds) {
            val batch = pacer.windowBytes
            val onWire = batch / linkBytesPerSec
            val step = maxOf(onWire, rtt)
            clock.advance(step)
            elapsed += step
            acked += batch
            pacer.observe(acked)
        }
        return Run(acked / elapsed, pacer.windowBytes)
    }

    @Test
    fun `floor spans several ack intervals so the stream never stops and waits`() {
        val (pacer, _) = makePacer()
        assertTrue(pacer.windowBytes >= 4 * TransferPacer.ACK_INTERVAL_BYTES)
        assertTrue(
            pacer.windowBytes / pacer.fragmentBytes >= 4,
            "at least four fragments must be in flight before the first ack is needed",
        )
    }

    @Test
    fun `reaches link rate over bluetooth`() {
        val link = 175_000.0
        val run = simulate(link, rtt = 0.25, seconds = 60.0)
        assertTrue(
            run.throughput > link * 0.9,
            "pacer must not be the constraint on a link this slow; got ${run.throughput.toInt()} B/s of ${link.toInt()}",
        )
    }

    @Test
    fun `reaches link rate when the round trip is long`() {
        val link = 175_000.0
        val run = simulate(link, rtt = 0.5, seconds = 120.0)
        assertTrue(run.throughput > link * 0.9, "got ${run.throughput.toInt()} B/s of ${link.toInt()}")
    }

    @Test
    fun `window stays inside the queueing budget`() {
        val link = 175_000.0
        val run = simulate(link, rtt = 0.25, seconds = 60.0)
        val queued = run.window / link
        assertTrue(queued <= TransferPacer.TARGET_DELAY_SECONDS * 1.5, "queued ${queued}s of link time")
    }

    @Test
    fun `window stays inside the daemons buffered depth`() {
        val run = simulate(20_000_000.0, rtt = 0.002, seconds = 5.0)
        assertTrue(run.window <= TransferPacer.MAX_WINDOW_BYTES)
        assertTrue(run.window / TransferPacer.FRAGMENT_BYTES <= 16)
    }

    @Test
    fun `a transient stall does not collapse the window`() {
        val (pacer, clock) = makePacer()
        var acked = 0L
        repeat(8) {
            clock.advance(0.25)
            acked += 44 * 1024
            pacer.observe(acked)
        }
        val settled = pacer.windowBytes
        assertTrue(settled > TransferPacer.MIN_WINDOW_BYTES)

        clock.advance(4.0)
        acked += 4 * 1024
        pacer.observe(acked)
        assertEquals(settled, pacer.windowBytes, "one slow sample must not shed the window")
    }

    @Test
    fun `sustained degradation does shrink the window`() {
        val (pacer, clock) = makePacer()
        var acked = 0L
        repeat(8) {
            clock.advance(0.25)
            acked += 128 * 1024
            pacer.observe(acked)
        }
        val fast = pacer.windowBytes
        repeat(TransferPacer.RATE_SAMPLE_COUNT) {
            clock.advance(2.0)
            acked += 8 * 1024
            pacer.observe(acked)
        }
        assertTrue(pacer.windowBytes < fast, "a link that is genuinely slow now must queue less")
        assertTrue(pacer.windowBytes >= TransferPacer.MIN_WINDOW_BYTES)
    }

    @Test
    fun `non-advancing ack is ignored`() {
        val (pacer, clock) = makePacer(startOffset = 8 * 1024)
        clock.advance(5.0)
        pacer.observe(8 * 1024)
        pacer.observe(4 * 1024)
        assertNull(pacer.ratePerSec, "no rate estimate without progress")
        assertEquals(TransferPacer.MIN_WINDOW_BYTES, pacer.windowBytes)
    }

    @Test
    fun `resume baseline does not invent a huge first sample`() {
        val (pacer, clock) = makePacer(startOffset = 30L * 1024 * 1024)
        clock.advance(0.25)
        pacer.observe(30L * 1024 * 1024 + 44 * 1024)
        val rate = pacer.ratePerSec ?: 0.0
        assertTrue(rate < 1_000_000, "rate came out as ${rate.toInt()} B/s, which means the baseline was 0")
    }
}
