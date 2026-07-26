package com.bridgething.companion

internal class TransferPacer(
    startOffset: Long = 0L,
    private val clock: () -> Double = { System.nanoTime() / 1_000_000_000.0 },
) {
    private var ackedBytes: Long = startOffset
    private var lastProgressAt: Double = clock()
    private val samples = ArrayDeque<Double>()

    val ratePerSec: Double?
        get() = samples.maxOrNull()

    val windowBytes: Long
        get() {
            val rate = ratePerSec ?: return MIN_WINDOW_BYTES
            val budget = (rate * TARGET_DELAY_SECONDS).toLong()
            return budget.coerceIn(MIN_WINDOW_BYTES, MAX_WINDOW_BYTES)
        }

    val fragmentBytes: Int get() = FRAGMENT_BYTES

    fun observe(acked: Long) {
        if (acked <= ackedBytes) return
        val now = clock()
        val dt = (now - lastProgressAt).coerceAtLeast(0.001)
        samples.addLast((acked - ackedBytes) / dt)
        while (samples.size > RATE_SAMPLE_COUNT) samples.removeFirst()
        ackedBytes = acked
        lastProgressAt = now
    }

    internal companion object {
        const val TARGET_DELAY_SECONDS: Double = 0.6

        const val ACK_INTERVAL_BYTES: Long = 16 * 1024L
        const val MIN_WINDOW_BYTES: Long = 4 * ACK_INTERVAL_BYTES

        const val MAX_WINDOW_BYTES: Long = 16 * ACK_INTERVAL_BYTES
        const val FRAGMENT_BYTES: Int = 16 * 1024
        const val RATE_SAMPLE_COUNT: Int = 8
    }
}
