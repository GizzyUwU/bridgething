package com.bridgething.companion

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.bridgething.companion.shell.LitertNluRunner
import java.io.File
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.BeforeClass
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.bridgething_companion.NluRejectionOutcome
import uniffi.bridgething_companion.NluRejectionPolicy
import uniffi.bridgething_companion.nluIntentCatalog
import uniffi.bridgething_companion.nluRejectionEvaluate

@RunWith(AndroidJUnit4::class)
class LitertGoldenTest {
    companion object {
        private val bundleDir = File("/data/local/tmp/bridgething-nlu")

        private const val LOGIT_TOLERANCE = 2.0f
        private const val SEQUENCE_LENGTH = 64

        private lateinit var fixtures: List<NluFixture>

        @BeforeClass
        @JvmStatic
        fun requireArtifacts() {
            val present = File(bundleDir, "model.tflite").isFile &&
                File(bundleDir, "manifest.json").isFile &&
                File(bundleDir, "fixtures.jsonl").isFile
            assumeTrue("push the bundle to ${bundleDir.path} to run the golden tier", present)
            fixtures = NluFixture.parseAll(File(bundleDir, "fixtures.jsonl").readText())
            assumeTrue("bundle carries no goldens", fixtures.isNotEmpty())
        }
    }

    private fun runner() = LitertNluRunner { bundleDir.path }

    @Test
    fun goldensAreFrozenAtTheModelSequenceLength() {
        for (fixture in fixtures) {
            assertEquals(fixture.utterance, SEQUENCE_LENGTH, fixture.inputIds.size)
            assertEquals(fixture.utterance, SEQUENCE_LENGTH, fixture.attentionMask.size)
        }
    }

    @Test
    fun headLogitsTrackGoldens() {
        val runner = runner()
        for (fixture in fixtures) {
            val out = runner.predict(fixture.inputIds, fixture.attentionMask)

            assertEquals(fixture.utterance, argmax(fixture.intentLogits), argmax(out.intentLogits))
            assertClose(fixture.utterance, fixture.intentLogits, out.intentLogits)
            assertTrue(
                "${fixture.utterance}: ood ${out.oodLogit} vs golden ${fixture.oodLogit}",
                abs(out.oodLogit - fixture.oodLogit) <= LOGIT_TOLERANCE,
            )

            assertEquals(fixture.utterance, fixture.closedLogits.size, out.closedLogits.size)
            fixture.closedLogits.forEachIndexed { head, golden ->
                assertClose("${fixture.utterance} closed_$head", golden, out.closedLogits[head])
            }

            val tags = fixture.bioLogits.size / fixture.inputIds.size
            assertEquals(fixture.utterance, fixture.bioLogits.size, out.bioLogits.size)
            for (position in fixture.attentionMask.indices.filter { fixture.attentionMask[it] == 1 }) {
                val range = position * tags until (position + 1) * tags
                val golden = fixture.bioLogits.slice(range)
                val actual = out.bioLogits.slice(range)
                assertEquals("${fixture.utterance} bio@$position", argmax(golden), argmax(actual))
                assertClose("${fixture.utterance} bio@$position", golden, actual)
            }
        }
    }

    @Test
    fun intentHeadIndexesTheCoreCatalog() {
        val names = nluIntentCatalog().surfaceNames
        val runner = runner()
        for (fixture in fixtures) {
            val out = runner.predict(fixture.inputIds, fixture.attentionMask)
            assertEquals(fixture.utterance, fixture.expectedIntent, names[argmax(out.intentLogits)])
        }
    }

    @Test
    fun inDomainGateSeparatesCommandsFromChatter() {
        val runner = runner()
        val command = fixtures.first { it.oodLogit < 0f }
        val chatter = fixtures.first { it.oodLogit > 0f }

        assertTrue(
            "${command.utterance} should be accepted",
            evaluate(runner, command) is NluRejectionOutcome.Accept,
        )
        assertEquals(
            NluRejectionOutcome.NoIntent,
            evaluate(runner, chatter),
        )
    }

    @Test
    fun prewarmLeavesTheRunnerReady() {
        val runner = runner()
        runner.prewarm()
        val fixture = fixtures.first()
        val out = runner.predict(fixture.inputIds, fixture.attentionMask)
        assertEquals(fixture.utterance, argmax(fixture.intentLogits), argmax(out.intentLogits))
    }

    private fun evaluate(runner: LitertNluRunner, fixture: NluFixture): NluRejectionOutcome {
        val out = runner.predict(fixture.inputIds, fixture.attentionMask)
        return nluRejectionEvaluate(
            out.intentLogits.map(Float::toDouble),
            -out.oodLogit.toDouble(),
            NluRejectionPolicy(),
        )
    }

    private fun <T : Comparable<T>> argmax(values: List<T>): Int =
        values.indices.maxByOrNull { values[it] } ?: -1

    private fun assertClose(label: String, golden: List<Float>, actual: List<Float>) {
        assertEquals(label, golden.size, actual.size)
        golden.indices.forEach {
            assertEquals("$label[$it]", golden[it], actual[it], LOGIT_TOLERANCE)
        }
    }
}
