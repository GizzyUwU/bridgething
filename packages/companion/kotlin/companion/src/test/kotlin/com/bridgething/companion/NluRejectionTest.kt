package com.bridgething.companion

import kotlin.math.abs
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows

class NluRejectionTest {
    private fun output(
        logits: Map<String, Double>,
        inDomain: Double = 8.0,
        count: Int? = null,
    ): NluInferenceOutput {
        val size = count ?: NluIntentCatalog.surfaceNames.size
        val vector = MutableList(size) { 0.0 }
        for ((name, logit) in logits) {
            val index = NluIntentCatalog.surfaceNames.indexOf(name)
            if (index in 0 until size) vector[index] = logit
        }
        return NluInferenceOutput(intentLogits = vector, inDomainLogit = inDomain)
    }

    @Test
    fun `a clear winner in domain is accepted`() {
        assertEquals(NluRejectionOutcome.Accept("PAUSE"), NluRejection.evaluate(output(mapOf("PAUSE" to 9.0))))
    }

    @Test
    fun `in-domain head below threshold yields NO_INTENT`() {
        assertEquals(
            NluRejectionOutcome.NoIntent,
            NluRejection.evaluate(output(mapOf("PAUSE" to 9.0), inDomain = -6.0)),
        )
    }

    @Test
    fun `out of domain outranks an ambiguous distribution`() {
        assertEquals(
            NluRejectionOutcome.NoIntent,
            NluRejection.evaluate(output(mapOf("PLAY" to 4.0, "SEARCH" to 4.0), inDomain = -6.0)),
        )
    }

    @Test
    fun `a narrow top-2 margin yields CLARIFY carrying the candidates`() {
        val outcome = NluRejection.evaluate(output(mapOf("PLAY" to 4.0, "SEARCH" to 3.95)))
        val clarify = outcome as? NluRejectionOutcome.Clarify ?: error("expected clarify, got $outcome")
        assertEquals(setOf("PLAY", "SEARCH"), clarify.alternates.toSet())
    }

    @Test
    fun `maxAlternates caps the candidate list`() {
        val policy = NluRejectionPolicy(clarifyMargin = 0.5, maxAlternates = 3)
        val outcome = NluRejection.evaluate(
            output(mapOf("PLAY" to 4.0, "SEARCH" to 4.0, "NEXT" to 4.0, "PAUSE" to 4.0)),
            policy,
        )
        val clarify = outcome as? NluRejectionOutcome.Clarify ?: error("expected clarify, got $outcome")
        assertEquals(3, clarify.alternates.size)
    }

    @Test
    fun `a widened margin turns an accepted intent into CLARIFY`() {
        val logits = output(mapOf("PLAY" to 4.0, "SEARCH" to 3.0))
        assertEquals(
            NluRejectionOutcome.Accept("PLAY"),
            NluRejection.evaluate(logits, NluRejectionPolicy(clarifyMargin = 0.01)),
        )
        assertTrue(
            NluRejection.evaluate(logits, NluRejectionPolicy(clarifyMargin = 0.9)) is NluRejectionOutcome.Clarify,
            "a 0.9 margin should not accept a 1.0-logit gap",
        )
    }

    @Test
    fun `a head that disagrees with the catalog throws rather than guessing`() {
        assertThrows<NluRejection.HeadMismatch> {
            NluRejection.evaluate(output(mapOf("PAUSE" to 9.0), count = 12))
        }
    }

    @Test
    fun `softmax is stable on large logits and sums to one`() {
        val probabilities = NluRejection.softmax(listOf(900.0, 901.0, 899.0))
        assertTrue(probabilities.all { it.isFinite() }, "softmax overflowed: $probabilities")
        assertTrue(abs(probabilities.sum() - 1) < 1e-9)
    }
}
