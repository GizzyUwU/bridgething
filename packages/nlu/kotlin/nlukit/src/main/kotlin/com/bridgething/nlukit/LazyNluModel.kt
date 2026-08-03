package com.bridgething.nlukit

class LazyNluModel(private val build: () -> NluModelRunning) : NluModelRunning, NluModelPrewarming {
    private val lock = Any()
    private var model: NluModelRunning? = null

    override fun prewarm() {
        resolved()
    }

    override fun predict(inputIds: List<Int>, attentionMask: List<Int>): NluModelOutputs =
        resolved().predict(inputIds, attentionMask)

    private fun resolved(): NluModelRunning = synchronized(lock) {
        model ?: build().also { model = it }
    }
}
