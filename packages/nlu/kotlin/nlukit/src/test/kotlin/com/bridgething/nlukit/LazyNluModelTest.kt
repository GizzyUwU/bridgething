package com.bridgething.nlukit

import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.DisplayName
import org.junit.jupiter.api.Test

class LazyNluModelTest {
    private val outputs = NluModelOutputs(listOf(1f), 0f, emptyList(), emptyList())

    @Test
    @DisplayName("construction does not build the runner")
    fun defersBuild() {
        var built = 0
        LazyNluModel {
            built += 1
            FakeNluModel(outputs)
        }
        assertEquals(0, built)
    }

    @Test
    @DisplayName("the runner is built once and reused across calls")
    fun buildsOnce() {
        var built = 0
        val model = LazyNluModel {
            built += 1
            FakeNluModel(outputs)
        }
        repeat(3) { model.predict(listOf(1), listOf(1)) }
        assertEquals(1, built)
    }

    @Test
    @DisplayName("prewarm pays the build so the first transcript does not")
    fun prewarmBuilds() {
        var built = 0
        val model = LazyNluModel {
            built += 1
            FakeNluModel(outputs)
        }
        model.prewarm()
        assertEquals(1, built)
        model.predict(listOf(1), listOf(1))
        assertEquals(1, built)
    }

    @Test
    @DisplayName("a concurrent first use waits rather than building twice")
    fun singleFlight() {
        val built = AtomicInteger()
        val entered = CountDownLatch(1)
        val model = LazyNluModel {
            built.incrementAndGet()
            entered.countDown()
            Thread.sleep(150)
            FakeNluModel(outputs)
        }

        val threads = 8
        val pool = Executors.newFixedThreadPool(threads)
        val start = CountDownLatch(1)
        val done = CountDownLatch(threads)
        repeat(threads) {
            pool.submit {
                start.await()
                model.predict(listOf(1), listOf(1))
                done.countDown()
            }
        }
        start.countDown()
        assertTrue(done.await(10, TimeUnit.SECONDS), "a caller never got through the lock")
        pool.shutdown()

        assertEquals(1, built.get())
    }

    @Test
    @DisplayName("a failed build is not cached")
    fun retriesFailedBuild() {
        var attempts = 0
        val model = LazyNluModel {
            attempts += 1
            if (attempts < 3) throw LitertNluModel.ModelError("weights not mapped yet")
            FakeNluModel(outputs)
        }

        repeat(2) {
            assertThrows(LitertNluModel.ModelError::class.java) { model.predict(listOf(1), listOf(1)) }
        }
        assertEquals(outputs, model.predict(listOf(1), listOf(1)))
        assertEquals(3, attempts)
    }
}
