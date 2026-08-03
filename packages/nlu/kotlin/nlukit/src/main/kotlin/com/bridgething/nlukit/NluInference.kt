package com.bridgething.nlukit

import com.bridgething.schema.NluSlots

data class NluInferenceOutput(
    val intentLogits: List<Double>,
    val inDomainLogit: Double,
    val slots: NluSlots = NluSlots(),
)

interface NluInferring {
    suspend fun infer(transcript: String): NluInferenceOutput
}

interface NluPrewarmable {
    suspend fun prewarm()
}

data class NluRejectionPolicy(
    val inDomainThreshold: Double = 0.5,
    val clarifyMargin: Double = 0.15,
    val maxAlternates: Int = 2,
)
