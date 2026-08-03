package com.bridgething.nlukit

import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import kotlin.math.abs
import kotlin.math.exp
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.BeforeClass
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.nlu.NluDecoder
import uniffi.nlu.SlotValue

@RunWith(AndroidJUnit4::class)
class LitertGoldenTest {
    companion object {
        private val bundleDir = File("/data/local/tmp/bridgething-nlu")
        private val modelFile = File(bundleDir, "model.tflite")

        private const val LOGIT_TOLERANCE = 2.0f

        private lateinit var fixtures: List<NluFixture>

        @BeforeClass
        @JvmStatic
        fun requireArtifacts() {
            val present = modelFile.isFile &&
                File(bundleDir, "manifest.json").isFile &&
                File(bundleDir, "tokenizer.json").isFile &&
                File(bundleDir, "fixtures.jsonl").isFile
            assumeTrue("push the bundle to ${bundleDir.path} to run the golden tier", present)
            fixtures = NluFixture.parseAll(File(bundleDir, "fixtures.jsonl").readText())
            assumeTrue("bundle carries no goldens", fixtures.isNotEmpty())
        }
    }

    @Test
    fun signatureExposesEveryHead() {
        LitertNluModel.load(modelFile).use { model ->
            assertEquals(64, model.sequenceLength)
            assertEquals(fixtures.first().closedLogits.size, model.closedHeadCount)
        }
    }

    @Test
    fun tokenizerMatchesGoldens() {
        NluDecoder.load(bundleDir.path).use { decoder ->
            for (fixture in fixtures) {
                val tokens = decoder.tokenize(fixture.utterance)
                assertEquals(fixture.utterance, fixture.inputIds, tokens.inputIds)
                assertEquals(fixture.utterance, fixture.attentionMask, tokens.attentionMask)
            }
        }
    }

    @Test
    fun headLogitsTrackGoldens() {
        LitertNluModel.load(modelFile).use { model ->
            for (fixture in fixtures) {
                val out = model.predict(fixture.inputIds, fixture.attentionMask)

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
    }

    @Test
    fun decodesGoldenFrames() = runBlocking {
        val inference = NluBundleInference.load(bundleDir, modelFile)
        for (fixture in fixtures) {
            val out = inference.infer(fixture.utterance)
            assertEquals(
                fixture.utterance,
                fixture.expectedIntent,
                NluIntentCatalog.name(argmax(out.intentLogits)),
            )
            val expected = NluSlotMapping.apply(fixture.expectedSlots.map { SlotValue(it.key, it.value) })
            assertEquals(fixture.utterance, expected, out.slots)
        }
    }

    @Test
    fun inDomainGateSeparatesCommandsFromChatter() = runBlocking {
        val inference = NluBundleInference.load(bundleDir, modelFile)
        val command = fixtures.first { it.oodLogit < 0f }
        val chatter = fixtures.first { it.oodLogit > 0f }

        val accepted = inference.infer(command.utterance)
        val rejected = inference.infer(chatter.utterance)

        assertTrue(
            "${command.utterance} scored ${accepted.inDomainLogit} in-domain",
            sigmoid(accepted.inDomainLogit) >= 0.5,
        )
        assertTrue(
            "${chatter.utterance} scored ${rejected.inDomainLogit} in-domain",
            sigmoid(rejected.inDomainLogit) < 0.5,
        )
    }

    @Test
    fun surfacesTheSweptOperatingPoint() {
        val rejection = NluBundleInference.load(bundleDir, modelFile).rejection
        assertNotNull("the bundle manifest carries a swept rejection point", rejection)
        assertEquals(0.5, rejection!!.inDomainThreshold, 1e-9)
        assertEquals(0.4, rejection.clarifyMargin, 1e-9)
    }

    @Test
    fun deferredModelStillInfers() = runBlocking {
        val inference = NluBundleInference.load(bundleDir, modelFile, deferModel = true)
        inference.prewarm()
        val fixture = fixtures.first()
        val out = inference.infer(fixture.utterance)
        assertEquals(fixture.expectedIntent, NluIntentCatalog.name(argmax(out.intentLogits)))
    }

    private fun <T : Comparable<T>> argmax(values: List<T>): Int =
        values.indices.maxByOrNull { values[it] } ?: -1

    private fun assertClose(label: String, golden: List<Float>, actual: List<Float>) {
        assertEquals(label, golden.size, actual.size)
        golden.indices.forEach {
            assertEquals("$label[$it]", golden[it], actual[it], LOGIT_TOLERANCE)
        }
    }

    private fun sigmoid(x: Double) = 1 / (1 + exp(-x))
}
