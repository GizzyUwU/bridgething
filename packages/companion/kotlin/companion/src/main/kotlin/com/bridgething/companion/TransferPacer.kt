package com.bridgething.companion

internal class TransferPacer(
    startOffset: Long = 0L,
    private val clock: () -> Double = { System.nanoTime() / 1_000_000_000.0 },
) {
    private var ackedBytes: Long = startOffset
    private var lastProgressAt: Double = clock()
    private var ratePerSec: Double? = null

    val windowBytes: Long
        get() {
            val rate = ratePerSec ?: return LARGE_FRAGMENT_BYTES.toLong()
            val bdp = (rate * TARGET_DELAY_SECONDS).toLong()
            return bdp.coerceIn(MIN_WINDOW_BYTES, MAX_WINDOW_BYTES)
        }

    val fragmentBytes: Int
        get() {
            ratePerSec ?: return LARGE_FRAGMENT_BYTES
            return if (windowBytes >= FRAGMENT_LADDER_BYTES) LARGE_FRAGMENT_BYTES else SMALL_FRAGMENT_BYTES
        }

    fun observe(acked: Long) {
        if (acked <= ackedBytes) return
        val now = clock()
        val dt = (now - lastProgressAt).coerceAtLeast(0.001)
        val instantaneous = (acked - ackedBytes) / dt
        ratePerSec = when {
            dt > 2 * TARGET_DELAY_SECONDS -> instantaneous
            else -> ratePerSec?.let { it + EWMA_ALPHA * (instantaneous - it) } ?: instantaneous
        }
        ackedBytes = acked
        lastProgressAt = now
    }

    internal companion object {
        const val TARGET_DELAY_SECONDS: Double = 0.6
        const val MIN_WINDOW_BYTES: Long = 4 * 1024L
        const val MAX_WINDOW_BYTES: Long = 64 * 1024L
        const val LARGE_FRAGMENT_BYTES: Int = 16 * 1024
        const val SMALL_FRAGMENT_BYTES: Int = 4 * 1024
        const val FRAGMENT_LADDER_BYTES: Long = 32 * 1024L
        private const val EWMA_ALPHA: Double = 0.3
    }
}
