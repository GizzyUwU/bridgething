package com.bridgething.companion

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.bridgething.asr.whisper.WhisperRecognizer
import com.bridgething.nlukit.NluBundleInference
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayVoiceMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeVoiceMsg
import com.bridgething.schema.MsgMeta
import com.bridgething.schema.NluStage
import com.bridgething.schema.VoiceCloseReason
import com.bridgething.schema.VoiceFrame
import com.bridgething.schema.VoiceStreamClose
import com.bridgething.schema.VoiceStreamOpen
import java.io.File
import java.util.UUID
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import com.bridgething.nlukit.NluInferring as KitNluInferring
import com.bridgething.nlukit.NluPrewarmable as KitNluPrewarmable

@RunWith(AndroidJUnit4::class)
class AndroidVoiceTurnTest {
    private val tag = "bridgething-voice-turn"

    @Test
    fun aSpokenTransportCommandDispatchesOnTheWire() = runBlocking {
        val weights = asrWeights()
        assumeTrue("push a ggml whisper model to ${ASR_DIR.path} to run the real turn", weights != null)
        assumeTrue("push the nlu bundle to ${NLU_DIR.path} to run the real turn", nluBundleStaged())

        val recognizer = WhisperSpeechRecognizer(WhisperRecognizer(modelPath = weights!!.path))
        val inference = NluBundleInference.load(NLU_DIR, File(NLU_DIR, "model.tflite"), deferModel = true)
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = InstrumentationRegistry.getInstrumentation().targetContext,
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "voice-turn-test", appVersion = "0.0.1", osName = "android"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
            voiceRecognizer = recognizer,
            nlu = KitInference(inference),
            nluRejection = inference.rejection?.let {
                NluRejectionPolicy(
                    inDomainThreshold = it.inDomainThreshold,
                    clarifyMargin = it.clarifyMargin,
                    maxAlternates = it.maxAlternates,
                )
            },
        )
        companion.start()

        val driver = WireDriver(adapter)
        driver.start(this)
        driver.connect()

        val streamId = UUID.randomUUID()
        driver.send(
            BridgeToGatewayMsgData.Voice(
                BridgeToGatewayVoiceMsg.StreamOpen(
                    VoiceStreamOpen(streamId = streamId, format = NextSongCommandFixture.format),
                ),
            ),
            MsgMeta.Event,
        )
        NextSongCommandFixture.packets.forEachIndexed { seq, packet ->
            driver.send(
                BridgeToGatewayMsgData.Voice(
                    BridgeToGatewayVoiceMsg.Frame(
                        VoiceFrame(streamId = streamId, seq = seq.toUInt(), packet = packet),
                    ),
                ),
                MsgMeta.Event,
            )
        }
        driver.send(
            BridgeToGatewayMsgData.Voice(
                BridgeToGatewayVoiceMsg.StreamClose(
                    VoiceStreamClose(streamId = streamId, reason = VoiceCloseReason.EndOfSpeech),
                ),
            ),
            MsgMeta.Event,
        )

        val frame = withTimeout(120.seconds) {
            driver.waitOutbound { out ->
                (out.data as? GatewayToBridgeMsgData.Voice)?.data is GatewayToBridgeVoiceMsg.Dispatch
            }
        }
        val dispatch = ((frame.data as GatewayToBridgeMsgData.Voice).data as GatewayToBridgeVoiceMsg.Dispatch).data

        Log.i(tag, "spoken     : ${NextSongCommandFixture.UTTERANCE}")
        Log.i(tag, "packets    : ${NextSongCommandFixture.packets.size}")
        Log.i(tag, "transcript : ${recognizer.lastTranscript}")
        Log.i(tag, "intent     : ${dispatch.resolved.intent}")
        Log.i(tag, "stage      : ${dispatch.stage}")

        assertTrue("whisper transcribed nothing", recognizer.lastTranscript.isNotBlank())
        assertEquals("NEXT", dispatch.resolved.intent)
        assertEquals(NluStage.FastPath, dispatch.stage)

        companion.stop()
    }

    private fun asrWeights(): File? =
        ASR_DIR.listFiles()?.firstOrNull { it.isFile && it.name.endsWith(".bin") }

    private fun nluBundleStaged(): Boolean =
        File(NLU_DIR, "model.tflite").isFile &&
            File(NLU_DIR, "manifest.json").isFile &&
            File(NLU_DIR, "tokenizer.json").isFile

    private companion object {
        val ASR_DIR = File("/data/local/tmp/bridgething-asr")
        val NLU_DIR = File("/data/local/tmp/bridgething-nlu")
    }
}

private class WhisperSpeechRecognizer(private val engine: WhisperRecognizer) : NluSpeechRecognizing {
    @Volatile
    var lastTranscript: String = ""
        private set

    override suspend fun prepare() {
        engine.prepare()
    }

    override suspend fun transcribe(samples: FloatArray, sampleRateHz: Int): String =
        engine.transcribe(samples, sampleRateHz).also { lastTranscript = it }
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
