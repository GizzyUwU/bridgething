package com.bridgething.companion

import com.bridgething.glue.BridgethingGlue
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayVoiceMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeVoiceMsg
import com.bridgething.schema.MsgMeta
import com.bridgething.schema.NluSlots
import com.bridgething.schema.NluStage
import com.bridgething.schema.VoiceCloseReason
import com.bridgething.schema.VoiceCodec
import com.bridgething.schema.VoiceDispatch
import com.bridgething.schema.VoiceFormat
import com.bridgething.schema.VoiceFrame
import com.bridgething.schema.VoiceStreamClose
import com.bridgething.schema.VoiceStreamOpen
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.UUID
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.seconds

class VoiceDispatchTest {
    private val format = VoiceFormat(codec = VoiceCodec.Opus, sampleRateHz = 16000u, channels = 1u)

    private class FakeRecognizer(
        private val transcript: String = "",
        private val failure: Throwable? = null,
    ) : NluSpeechRecognizing {
        val prepared = AtomicInteger(0)
        val transcribed = CopyOnWriteArrayList<Int>()

        override suspend fun prepare() {
            prepared.incrementAndGet()
        }

        override suspend fun transcribe(samples: FloatArray, sampleRateHz: Int): String {
            transcribed.add(samples.size)
            failure?.let { throw it }
            return transcript
        }
    }

    private class FakeDecoder : VoicePacketDecoding {
        val turns = CopyOnWriteArrayList<List<ByteArray>>()

        override suspend fun decode(packets: List<ByteArray>, format: VoiceFormat): FloatArray {
            turns.add(packets.toList())
            return FloatArray(packets.size * SAMPLES_PER_PACKET)
        }
    }

    private class FakeCatalogResolver(
        private val uri: String? = null,
        private val contextUri: String? = null,
        private val failure: Throwable? = null,
    ) : VoiceCatalogResolving {
        override suspend fun decorate(prediction: NluPrediction): NluPrediction {
            failure?.let { throw it }
            prediction.slots.uri = uri
            prediction.slots.contextUri = contextUri
            return prediction
        }
    }

    private class FakeVoiceGlue(
        private val resolver: VoiceCatalogResolving,
        glue: BridgethingGlue = FakeGlue(),
    ) : BridgethingGlue by glue, VoiceCatalogProviding {
        override fun voiceResolver(): VoiceCatalogResolving = resolver
    }

    private suspend fun boot(
        scope: CoroutineScope,
        recognizer: NluSpeechRecognizing,
        decoder: VoicePacketDecoding = FakeDecoder(),
        nlu: NluInferring? = null,
        glue: BridgethingGlue? = null,
    ): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "voice-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
            voiceRecognizer = recognizer,
            voiceDecoder = decoder,
            nlu = nlu,
        )
        glue?.let { companion.attach(it) }
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    private suspend fun turn(driver: WireDriver, frames: List<Pair<UInt, ByteArray>> = defaultFrames()) {
        val streamId = UUID.randomUUID()
        driver.send(
            BridgeToGatewayMsgData.Voice(
                BridgeToGatewayVoiceMsg.StreamOpen(VoiceStreamOpen(streamId = streamId, format = format)),
            ),
            MsgMeta.Event,
        )
        for ((seq, packet) in frames) {
            driver.send(
                BridgeToGatewayMsgData.Voice(
                    BridgeToGatewayVoiceMsg.Frame(VoiceFrame(streamId = streamId, seq = seq, packet = packet)),
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
    }

    private suspend fun awaitDispatch(driver: WireDriver): VoiceDispatch {
        val msg = withTimeout(5.seconds) {
            driver.waitOutbound { frame ->
                (frame.data as? GatewayToBridgeMsgData.Voice)?.data is GatewayToBridgeVoiceMsg.Dispatch
            }
        }
        return ((msg.data as GatewayToBridgeMsgData.Voice).data as GatewayToBridgeVoiceMsg.Dispatch).data
    }

    private fun playInference() = FakeNluInference(
        logits = mapOf("PLAY" to 9.0),
        slots = NluSlots(target = "hounds of love"),
    )

    @Test
    fun `a fast-path turn resolves and dispatches on the wire`() = runBlocking {
        val (companion, driver) = boot(this, FakeRecognizer("pause"))
        turn(driver)
        val dispatch = awaitDispatch(driver)
        assertEquals("PAUSE", dispatch.resolved.intent)
        assertEquals(NluStage.FastPath, dispatch.stage)
        companion.stop()
    }

    @Test
    fun `a fast-path miss resolves through the injected model`() = runBlocking {
        val (companion, driver) = boot(this, FakeRecognizer(CONTENT_UTTERANCE), nlu = playInference())
        turn(driver)
        val dispatch = awaitDispatch(driver)
        assertEquals("PLAY", dispatch.resolved.intent)
        assertEquals(NluStage.Model, dispatch.stage)
        assertEquals("hounds of love", dispatch.resolved.slots.target)
        companion.stop()
    }

    @Test
    fun `the resolver decorates the turn before it goes on the wire`() = runBlocking {
        val (companion, driver) = boot(
            this,
            FakeRecognizer(CONTENT_UTTERANCE),
            nlu = playInference(),
            glue = FakeVoiceGlue(FakeCatalogResolver(uri = "spotify:track:7", contextUri = "spotify:album:2")),
        )
        turn(driver)
        val dispatch = awaitDispatch(driver)
        assertEquals("hounds of love", dispatch.resolved.slots.target)
        assertEquals("spotify:track:7", dispatch.resolved.slots.uri)
        assertEquals("spotify:album:2", dispatch.resolved.slots.contextUri)
        companion.stop()
    }

    @Test
    fun `no resolver still dispatches, with the slots left unresolved`() = runBlocking {
        val (companion, driver) = boot(this, FakeRecognizer(CONTENT_UTTERANCE), nlu = playInference())
        turn(driver)
        val dispatch = awaitDispatch(driver)
        assertEquals("PLAY", dispatch.resolved.intent)
        assertNull(dispatch.resolved.slots.uri)
        companion.stop()
    }

    @Test
    fun `a failing resolver still dispatches so the daemon answers the turn`() = runBlocking {
        val (companion, driver) = boot(
            this,
            FakeRecognizer(CONTENT_UTTERANCE),
            nlu = playInference(),
            glue = FakeVoiceGlue(FakeCatalogResolver(failure = IllegalStateException("offline"))),
        )
        turn(driver)
        val dispatch = awaitDispatch(driver)
        assertEquals("PLAY", dispatch.resolved.intent)
        assertEquals("hounds of love", dispatch.resolved.slots.target)
        assertNull(dispatch.resolved.slots.uri, "a failed resolution must not invent a uri")
        companion.stop()
    }

    @Test
    fun `a failing recognizer dispatches a no-intent turn rather than silence`() = runBlocking {
        val recognizer = FakeRecognizer(failure = IllegalStateException("model missing"))
        val (companion, driver) = boot(this, recognizer, nlu = playInference())
        turn(driver)
        val dispatch = awaitDispatch(driver)
        assertEquals(NluIntentCatalog.NO_INTENT, dispatch.resolved.intent)
        assertEquals(NluStage.RejectedNoIntent, dispatch.stage)
        assertEquals(1, recognizer.transcribed.size, "the recognizer really was asked")
        companion.stop()
    }

    @Test
    fun `packets reach the decoder in sequence order however they arrived`() = runBlocking {
        val decoder = FakeDecoder()
        val (companion, driver) = boot(this, FakeRecognizer("pause"), decoder = decoder)
        val frames = listOf(2u to packet(2), 0u to packet(0), 1u to packet(1))
        turn(driver, frames)
        awaitDispatch(driver)

        assertEquals(1, decoder.turns.size)
        assertEquals(
            listOf(0, 1, 2),
            decoder.turns.first().map { it.first().toInt() },
            "reassembly by seq has to reproduce the capture order exactly",
        )
        companion.stop()
    }

    @Test
    fun `prewarm fires once on the first stream open, not once per turn`() = runBlocking {
        val inference = PrewarmableNluInference()
        val (companion, driver) = boot(this, FakeRecognizer("pause"), nlu = inference)
        turn(driver)
        awaitDispatch(driver)
        turn(driver)
        awaitDispatch(driver)

        assertEquals(1, inference.warmed.get(), "a warm model is not re-warmed by the next turn")
        companion.stop()
    }

    @Test
    fun `a turn with no packets never reaches the decoder`() = runBlocking {
        val decoder = FakeDecoder()
        val recognizer = FakeRecognizer("pause")
        val (companion, driver) = boot(this, recognizer, decoder = decoder)
        turn(driver, frames = emptyList())
        val dispatch = awaitDispatch(driver)

        assertTrue(decoder.turns.isEmpty(), "there was nothing to decode")
        assertTrue(recognizer.transcribed.isEmpty(), "there was nothing to transcribe")
        assertEquals(NluIntentCatalog.NO_INTENT, dispatch.resolved.intent)
        companion.stop()
    }

    @Test
    fun `the recognizer is prepared when capture starts`() = runBlocking {
        val recognizer = FakeRecognizer("pause")
        val (companion, driver) = boot(this, recognizer)
        turn(driver)
        awaitDispatch(driver)
        assertEquals(1, recognizer.prepared.get())
        companion.stop()
    }

    private companion object {
        const val CONTENT_UTTERANCE = "play the new mitski album"
        const val SAMPLES_PER_PACKET = 320

        fun packet(seq: Int): ByteArray = byteArrayOf(seq.toByte(), 0x1f, 0x2e)

        fun defaultFrames(): List<Pair<UInt, ByteArray>> = listOf(0u to packet(0), 1u to packet(1))
    }
}
