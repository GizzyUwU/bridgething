package com.bridgething.nlukit

import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.nlu.NluDecoder
import uniffi.nlu.NluDecoderInterface

class NluBundleInference(
    private val decoder: NluDecoderInterface,
    private val model: NluModelRunning,
) : NluInferring, NluPrewarmable, AutoCloseable {
    class CatalogMismatch(bundle: List<String>, catalog: List<String>) :
        Exception("bundle intents $bundle do not match the companion catalog $catalog")

    val rejection: NluRejectionPolicy?

    init {
        val info = decoder.info()
        if (info.intentNames != NluIntentCatalog.surfaceNames) {
            throw CatalogMismatch(info.intentNames, NluIntentCatalog.surfaceNames)
        }
        rejection = info.rejection?.let {
            NluRejectionPolicy(inDomainThreshold = it.inDomainThreshold, clarifyMargin = it.clarifyMargin)
        }
    }

    override suspend fun infer(transcript: String): NluInferenceOutput = withContext(Dispatchers.Default) {
        val tokens = decoder.tokenize(transcript)
        val out = model.predict(tokens.inputIds, tokens.attentionMask)
        val frame = decoder.decode(transcript, tokens, out.intentLogits, out.bioLogits, out.closedLogits)
        NluInferenceOutput(
            intentLogits = out.intentLogits.map(Float::toDouble),
            inDomainLogit = -out.oodLogit.toDouble(),
            slots = NluSlotMapping.apply(frame.slots),
        )
    }

    override fun close() {
        (model as? AutoCloseable)?.close()
        (decoder as? AutoCloseable)?.close()
    }

    override suspend fun prewarm() = withContext(Dispatchers.Default) {
        (model as? NluModelPrewarming)?.prewarm()
        Unit
    }

    companion object {
        fun load(
            bundleDir: File,
            modelFile: File = File(bundleDir, "model.tflite"),
            threads: Int = 2,
            deferModel: Boolean = false,
        ): NluBundleInference {
            val decoder = NluDecoder.load(bundleDir.path)
            val build = { LitertNluModel.load(modelFile, threads) }
            return NluBundleInference(decoder, if (deferModel) LazyNluModel(build) else build())
        }
    }
}
