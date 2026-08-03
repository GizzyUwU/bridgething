package com.bridgething.companion

import kotlin.math.exp

sealed interface NluRejectionOutcome {
    data class Accept(val intent: String) : NluRejectionOutcome
    data object NoIntent : NluRejectionOutcome
    data class Clarify(val alternates: List<String>) : NluRejectionOutcome
}

object NluRejection {
    class HeadMismatch(logits: Int, catalog: Int) :
        Exception("intent head emits $logits logits but the catalog has $catalog names")

    fun evaluate(
        output: NluInferenceOutput,
        policy: NluRejectionPolicy = NluRejectionPolicy(),
    ): NluRejectionOutcome {
        val names = NluIntentCatalog.surfaceNames
        if (output.intentLogits.size != names.size) {
            throw HeadMismatch(output.intentLogits.size, names.size)
        }

        if (sigmoid(output.inDomainLogit) < policy.inDomainThreshold) return NluRejectionOutcome.NoIntent

        val ranked = softmax(output.intentLogits)
            .mapIndexed { index, probability -> names[index] to probability }
            .sortedByDescending { it.second }

        val top = ranked.firstOrNull() ?: return NluRejectionOutcome.NoIntent
        val runnerUp = ranked.getOrNull(1) ?: return NluRejectionOutcome.Accept(top.first)

        if (top.second - runnerUp.second < policy.clarifyMargin) {
            return NluRejectionOutcome.Clarify(ranked.take(maxOf(policy.maxAlternates, 0)).map { it.first })
        }
        return NluRejectionOutcome.Accept(top.first)
    }

    fun sigmoid(x: Double): Double = 1 / (1 + exp(-x))

    fun softmax(logits: List<Double>): List<Double> {
        val peak = logits.maxOrNull() ?: return emptyList()
        val exps = logits.map { exp(it - peak) }
        val total = exps.sum()
        if (total <= 0) return List(logits.size) { 0.0 }
        return exps.map { it / total }
    }
}
