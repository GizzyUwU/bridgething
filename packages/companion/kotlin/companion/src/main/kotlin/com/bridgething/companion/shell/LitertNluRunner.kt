package com.bridgething.companion.shell

import java.io.File
import uniffi.bridgething_companion.NluModelOutputs
import uniffi.bridgething_companion.NluModelRunner
import uniffi.bridgething_companion.NluRunnerException

public class LitertNluRunner(
    private val bundleDir: () -> String?,
) : NluModelRunner {
    private val lock = Any()
    private var loaded: Pair<String, LitertNluModel>? = null

    override fun prewarm() {
        runCatching { resolved() }
    }

    override fun predict(inputIds: List<Int>, attentionMask: List<Int>): NluModelOutputs {
        val model = resolved() ?: throw NluRunnerException.NotLoaded()
        return try {
            model.predict(inputIds, attentionMask)
        } catch (t: Throwable) {
            throw NluRunnerException.Failed(t.message ?: t.toString())
        }
    }

    private fun resolved(): LitertNluModel? = synchronized(lock) {
        val dir = bundleDir() ?: return null
        loaded?.let { (heldDir, model) ->
            if (heldDir == dir) return model
            runCatching { model.close() }
        }
        val model = try {
            LitertNluModel.load(File(dir, MODEL_FILE))
        } catch (t: Throwable) {
            throw NluRunnerException.Failed(t.message ?: t.toString())
        }
        loaded = dir to model
        model
    }

    private companion object {
        const val MODEL_FILE = "model.tflite"
    }
}
