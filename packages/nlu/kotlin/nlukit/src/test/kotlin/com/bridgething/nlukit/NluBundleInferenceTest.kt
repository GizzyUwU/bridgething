package com.bridgething.nlukit

import com.bridgething.schema.NluDirection
import com.bridgething.schema.NluTargetType
import kotlin.math.exp
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.DisplayName
import org.junit.jupiter.api.Test
import uniffi.nlu.DecodedFrame
import uniffi.nlu.Rejection

class NluBundleInferenceTest {
    private fun outputs(ood: Float = -8f, intents: Int = NluIntentCatalog.surfaceNames.size) = NluModelOutputs(
        intentLogits = List(intents) { 0f },
        oodLogit = ood,
        bioLogits = emptyList(),
        closedLogits = emptyList(),
    )

    @Test
    @DisplayName("in-domain logit is the negated ood head")
    fun negatesOodHead() = runBlocking {
        val model = FakeNluModel(outputs(ood = -8f))
        val out = NluBundleInference(FakeNluDecoder(), model).infer("play some jazz")
        assertEquals(8.0, out.inDomainLogit, 1e-6)
    }

    @Test
    @DisplayName("ood sign survives the fixture that is out of domain")
    fun oodSignDecidesFixtures() = runBlocking {
        val fixtures = NluFixtures.load()
        assumeTrue(fixtures.isNotEmpty(), "bundle fixtures not present")

        val inDomain = fixtures.first { it.utterance == "play some jazz" }
        val outOfDomain = fixtures.first { it.utterance == "what is the capital of peru" }
        assertTrue(inDomain.oodLogit < 0f, "fixture no longer exercises the in-domain side")
        assertTrue(outOfDomain.oodLogit > 0f, "fixture no longer exercises the out-of-domain side")

        val accepted = NluBundleInference(FakeNluDecoder(), FakeNluModel(inDomain.asModelOutputs()))
            .infer(inDomain.utterance)
        val rejected = NluBundleInference(FakeNluDecoder(), FakeNluModel(outOfDomain.asModelOutputs()))
            .infer(outOfDomain.utterance)

        assertTrue(sigmoid(accepted.inDomainLogit) >= 0.5, "a command must clear the in-domain threshold")
        assertTrue(sigmoid(rejected.inDomainLogit) < 0.5, "a non-command must fall below the in-domain threshold")
    }

    @Test
    @DisplayName("tokens and every head reach the decoder unchanged")
    fun decodePlumbing() = runBlocking {
        val fixtures = NluFixtures.load()
        assumeTrue(fixtures.isNotEmpty(), "bundle fixtures not present")
        val fixture = fixtures.first { it.utterance == "play the album 1989 by taylor swift" }

        val decoder = FakeNluDecoder(
            frame = DecodedFrame("PLAY", slots("target" to "1989 by taylor swift", "target_type" to "album")),
        )
        val model = FakeNluModel(fixture.asModelOutputs())
        val out = NluBundleInference(decoder, model).infer(fixture.utterance)

        assertEquals(fixture.utterance, decoder.tokenized)
        assertEquals(fixture.utterance, decoder.decodedTranscript)
        assertEquals(fixture.intentLogits, decoder.decodedIntentLogits)
        assertEquals(fixture.bioLogits, decoder.decodedBioLogits)
        assertEquals(fixture.closedLogits, decoder.decodedClosedLogits)
        assertEquals(fixture.intentLogits.map(Float::toDouble), out.intentLogits)
        assertEquals("1989 by taylor swift", out.slots.target)
        assertEquals(NluTargetType.Album, out.slots.targetType)
    }

    @Test
    @DisplayName("tokenizer output is what the runner sees")
    fun forwardsTokens() = runBlocking {
        val model = FakeNluModel(outputs())
        val decoder = FakeNluDecoder()
        NluBundleInference(decoder, model).infer("turn it up")
        assertEquals(decoder.tokenize("turn it up").inputIds, model.sawInputIds)
        assertEquals(decoder.tokenize("turn it up").attentionMask, model.sawAttentionMask)
    }

    @Test
    @DisplayName("decoded slots land on the wire shape")
    fun mapsSlots() = runBlocking {
        val decoder = FakeNluDecoder(frame = DecodedFrame("SET_VOLUME", slots("direction" to "up")))
        val out = NluBundleInference(decoder, FakeNluModel(outputs())).infer("turn it up")
        assertEquals(NluDirection.Up, out.slots.direction)
    }

    @Test
    @DisplayName("a bundle whose intents are not the catalog is refused")
    fun refusesForeignCatalog() {
        val decoder = FakeNluDecoder(intentNames = NluIntentCatalog.surfaceNames.dropLast(1))
        val error = assertThrows(NluBundleInference.CatalogMismatch::class.java) {
            NluBundleInference(decoder, FakeNluModel(outputs()))
        }
        assertTrue(error.message!!.contains("do not match the companion catalog"))
    }

    @Test
    @DisplayName("intent order is part of the contract, not just membership")
    fun refusesReorderedCatalog() {
        val decoder = FakeNluDecoder(intentNames = NluIntentCatalog.surfaceNames.reversed())
        assertThrows(NluBundleInference.CatalogMismatch::class.java) {
            NluBundleInference(decoder, FakeNluModel(outputs()))
        }
    }

    @Test
    @DisplayName("the bundle's calibrated operating point is surfaced")
    fun surfacesRejection() {
        val decoder = FakeNluDecoder(rejection = Rejection(inDomainThreshold = 0.62, clarifyMargin = 0.4))
        val rejection = NluBundleInference(decoder, FakeNluModel(outputs())).rejection
        assertNotNull(rejection)
        assertEquals(0.62, rejection!!.inDomainThreshold, 1e-9)
        assertEquals(0.4, rejection.clarifyMargin, 1e-9)
        assertEquals(2, rejection.maxAlternates)
    }

    @Test
    @DisplayName("an export without a sweep surfaces no operating point")
    fun toleratesMissingRejection() {
        val decoder = FakeNluDecoder(rejection = null)
        assertNull(NluBundleInference(decoder, FakeNluModel(outputs())).rejection)
    }

    @Test
    @DisplayName("prewarm reaches a runner that can warm and skips one that cannot")
    fun prewarmDelegates() = runBlocking {
        var built = 0
        val lazy = LazyNluModel {
            built += 1
            FakeNluModel(outputs())
        }
        NluBundleInference(FakeNluDecoder(), lazy).prewarm()
        assertEquals(1, built)

        NluBundleInference(FakeNluDecoder(), FakeNluModel(outputs())).prewarm()
    }

    private fun sigmoid(x: Double) = 1 / (1 + exp(-x))
}
