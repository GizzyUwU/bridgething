package com.bridgething

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.util.Log
import com.bridgething.asr.whisper.WhisperRecognizer
import com.bridgething.companion.ModelBundleKind
import com.bridgething.companion.ModelBundleState
import com.bridgething.companion.ModelBundleStore
import com.bridgething.companion.ModelTransferPolicy
import com.bridgething.companion.NluInferenceOutput
import com.bridgething.companion.NluInferring
import com.bridgething.companion.NluPrewarmable
import com.bridgething.companion.NluRejectionPolicy
import com.bridgething.companion.NluSpeechRecognizing
import com.bridgething.nlukit.NluBundleInference
import java.io.File
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import com.bridgething.nlukit.NluInferring as KitNluInferring
import com.bridgething.nlukit.NluPrewarmable as KitNluPrewarmable

internal object VoiceModels {
    private const val TAG = "BridgethingVoice"

    private val GGML_MAGIC = byteArrayOf(0x6c, 0x6d, 0x67, 0x67)

    class LoadedNlu(val client: NluInferring, val rejection: NluRejectionPolicy?)

    private class Stores(val nlu: ModelBundleStore, val asr: ModelBundleStore)

    @Volatile
    private var stores: Stores? = null

    suspend fun ensure(context: Context) {
        val s = stores(context)
        s.nlu.ensure()
        s.asr.ensure()
        Log.i(TAG, "models: nlu=${s.nlu.state} asr=${s.asr.state}")
    }

    suspend fun setEnabled(context: Context, value: Boolean) {
        val s = stores(context)
        s.nlu.setEnabled(value)
        s.asr.setEnabled(value)
    }

    fun state(context: Context): ModelBundleState {
        val s = stores(context)
        return merge(s.nlu.state, s.asr.state)
    }

    fun states(context: Context): Flow<ModelBundleState> {
        val s = stores(context)
        return combine(s.nlu.states, s.asr.states, ::merge)
    }

    private fun merge(nlu: ModelBundleState, asr: ModelBundleState): ModelBundleState {
        val parts = listOf(nlu, asr)
        val downloading = parts.filterIsInstance<ModelBundleState.Downloading>()
        if (downloading.isNotEmpty()) {
            return ModelBundleState.Downloading(
                received = downloading.sumOf { it.received },
                total = downloading.sumOf { it.total },
            )
        }
        parts.filterIsInstance<ModelBundleState.Failed>().firstOrNull()?.let { return it }
        val ready = parts.filterIsInstance<ModelBundleState.Ready>()
        if (ready.size != parts.size) return ModelBundleState.Absent
        val versions = ready.map { it.version }.distinct()
        return ModelBundleState.Ready(versions.singleOrNull() ?: versions.joinToString(" + "))
    }

    fun recognizer(context: Context): NluSpeechRecognizing? {
        val weights = stores(context).asr.live ?: return null
        return WhisperSpeechRecognizer(WhisperRecognizer(modelPath = weights.path))
    }

    fun inference(context: Context): LoadedNlu? {
        val bundle = stores(context).nlu.live ?: return null
        val loaded = runCatching { NluBundleInference.load(bundle, deferModel = true) }
            .onFailure { Log.w(TAG, "installed nlu bundle failed to load: ${it.message}") }
            .getOrNull() ?: return null
        return LoadedNlu(KitInference(loaded), loaded.rejection?.let(::toCompanionRejection))
    }

    @Synchronized
    private fun stores(context: Context): Stores {
        stores?.let { return it }
        val app = context.applicationContext
        val config = ModelBundleStore.Config(storageDirectory = app.filesDir)
        val enabled = HybridBridgethingSessionImpl.voiceModelEnabled(app)
        val policy = unmeteredPolicy(app)
        val built = Stores(
            nlu = ModelBundleStore(
                kind = ModelBundleKind.Nlu,
                config = config,
                enabled = enabled,
                transferPolicy = policy,
                validator = { dir -> NluBundleInference.load(dir).use { } },
            ),
            asr = ModelBundleStore(
                kind = ModelBundleKind.Asr,
                config = config,
                enabled = enabled,
                transferPolicy = policy,
                validator = { file -> requireGgml(file) },
            ),
        )
        stores = built
        return built
    }

    private fun unmeteredPolicy(context: Context): ModelTransferPolicy = ModelTransferPolicy {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val caps = manager?.activeNetwork?.let { manager.getNetworkCapabilities(it) }
        caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true
    }

    private fun requireGgml(file: File) {
        val head = ByteArray(GGML_MAGIC.size)
        val read = file.inputStream().use { it.read(head) }
        if (read != head.size || !head.contentEquals(GGML_MAGIC)) {
            throw IllegalStateException("asr model does not open with a ggml header")
        }
    }

    private fun toCompanionRejection(policy: com.bridgething.nlukit.NluRejectionPolicy): NluRejectionPolicy =
        NluRejectionPolicy(
            inDomainThreshold = policy.inDomainThreshold,
            clarifyMargin = policy.clarifyMargin,
            maxAlternates = policy.maxAlternates,
        )
}

private class WhisperSpeechRecognizer(private val engine: WhisperRecognizer) : NluSpeechRecognizing {
    override suspend fun prepare() {
        engine.prepare()
    }

    override suspend fun transcribe(samples: FloatArray, sampleRateHz: Int): String =
        engine.transcribe(samples, sampleRateHz)
}

private class KitInference(private val inner: KitNluInferring) : NluInferring, NluPrewarmable {
    override suspend fun prewarm() {
        (inner as? KitNluPrewarmable)?.prewarm()
    }

    override suspend fun infer(transcript: String): NluInferenceOutput {
        val out = inner.infer(transcript)
        return NluInferenceOutput(
            intentLogits = out.intentLogits,
            inDomainLogit = out.inDomainLogit,
            slots = out.slots,
        )
    }
}
