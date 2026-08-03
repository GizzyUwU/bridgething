package com.bridgething.nlukit

data class NluModelOutputs(
    val intentLogits: List<Float>,
    val oodLogit: Float,
    val bioLogits: List<Float>,
    val closedLogits: List<List<Float>>,
)

interface NluModelRunning {
    fun predict(inputIds: List<Int>, attentionMask: List<Int>): NluModelOutputs
}

interface NluModelPrewarming {
    fun prewarm()
}
