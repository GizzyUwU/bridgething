package com.bridgething.companion

import com.bridgething.schema.NluSlots
import java.util.concurrent.atomic.AtomicInteger

class FakeNluInference(
    private val logits: Map<String, Double> = emptyMap(),
    private val inDomainLogit: Double = 8.0,
    private val slots: NluSlots = NluSlots(),
    private val failWithCall: Boolean = false,
    private val logitCountOverride: Int? = null,
) : NluInferring {
    class ShouldNotHaveBeenCalled : Exception("the model ran on an utterance the caller should have claimed")

    override suspend fun infer(transcript: String): NluInferenceOutput {
        if (failWithCall) throw ShouldNotHaveBeenCalled()
        val count = logitCountOverride ?: NluIntentCatalog.surfaceNames.size
        val vector = MutableList(count) { 0.0 }
        for ((name, logit) in logits) {
            val index = NluIntentCatalog.surfaceNames.indexOf(name)
            if (index in 0 until count) vector[index] = logit
        }
        return NluInferenceOutput(intentLogits = vector, inDomainLogit = inDomainLogit, slots = slots)
    }
}

class PrewarmableNluInference : NluInferring, NluPrewarmable {
    val warmed = AtomicInteger(0)

    override suspend fun prewarm() {
        warmed.incrementAndGet()
    }

    override suspend fun infer(transcript: String): NluInferenceOutput =
        NluInferenceOutput(
            intentLogits = List(NluIntentCatalog.surfaceNames.size) { 0.0 },
            inDomainLogit = 8.0,
        )
}
