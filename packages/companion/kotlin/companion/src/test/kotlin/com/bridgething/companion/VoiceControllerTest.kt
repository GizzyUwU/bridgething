package com.bridgething.companion

import com.bridgething.schema.NluSlots
import com.bridgething.schema.NluStage
import com.bridgething.schema.NluTargetType
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows

class VoiceControllerTest {
    @Test
    fun `fast path short-circuits before the model runs`() = runBlocking {
        val controller = VoiceController(FakeNluInference(failWithCall = true))
        val resolution = controller.resolve("Pause.")
        assertEquals(NluStage.FastPath, resolution.stage)
        assertEquals("PAUSE", resolution.resolved.intent)
    }

    @Test
    fun `an empty transcript is NO_INTENT without touching the model`() = runBlocking {
        val controller = VoiceController(FakeNluInference(failWithCall = true))
        val resolution = controller.resolve("   ")
        assertEquals(NluStage.RejectedNoIntent, resolution.stage)
        assertEquals("NO_INTENT", resolution.resolved.intent)
    }

    @Test
    fun `with no model configured the fast path still resolves`() = runBlocking {
        val resolution = VoiceController().resolve("turn it up")
        assertEquals(NluStage.FastPath, resolution.stage)
        assertEquals("SET_VOLUME", resolution.resolved.intent)
    }

    @Test
    fun `with no model configured a fast-path miss says so rather than guessing`() = runBlocking {
        val resolution = VoiceController().resolve("play the new mitski album")
        assertEquals(NluStage.NoModel, resolution.stage)
        assertEquals("NO_INTENT", resolution.resolved.intent)
        assertEquals("play the new mitski album", resolution.resolved.transcript)
    }

    @Test
    fun `an accepted intent carries the decoded slots through to the wire`() = runBlocking {
        val client = FakeNluInference(
            logits = mapOf("PLAY" to 9.0),
            slots = NluSlots(target = "you stupid bitch by girl in red", targetType = NluTargetType.Track),
        )
        val resolution = VoiceController(client).resolve("play you stupid bitch by girl in red")
        assertEquals(NluStage.Model, resolution.stage)
        assertEquals("PLAY", resolution.resolved.intent)
        assertEquals("you stupid bitch by girl in red", resolution.resolved.slots.target)
        assertEquals(NluTargetType.Track, resolution.resolved.slots.targetType)
    }

    @Test
    fun `prewarm reaches a client that can be warmed`() = runBlocking {
        val client = PrewarmableNluInference()
        VoiceController(client).prewarm()
        assertEquals(1, client.warmed.get())
    }

    @Test
    fun `prewarm is a no-op for a client with nothing to warm`() = runBlocking {
        VoiceController(FakeNluInference(failWithCall = true)).prewarm()
        VoiceController().prewarm()
    }

    @Test
    fun `an out-of-domain utterance resolves to NO_INTENT`() = runBlocking {
        val client = FakeNluInference(logits = mapOf("SEARCH" to 5.0), inDomainLogit = -6.0)
        val resolution = VoiceController(client).resolve("what is the capital of peru")
        assertEquals(NluStage.RejectedNoIntent, resolution.stage)
        assertEquals("NO_INTENT", resolution.resolved.intent)
    }

    @Test
    fun `an ambiguous utterance resolves to CLARIFY with alternates and no slots`() = runBlocking {
        val client = FakeNluInference(
            logits = mapOf("PLAY" to 4.0, "SEARCH" to 3.95),
            slots = NluSlots(target = "pink"),
        )
        val resolution = VoiceController(client).resolve("pink")
        assertEquals(NluStage.RejectedClarify, resolution.stage)
        assertEquals("CLARIFY", resolution.resolved.intent)
        assertEquals(setOf("PLAY", "SEARCH"), resolution.resolved.alternates.orEmpty().map { it.intent }.toSet())
        assertTrue(resolution.resolved.alternates.orEmpty().all { it.slots == null })
    }

    @Test
    fun `the transcript rides along on every outcome`() = runBlocking {
        val client = FakeNluInference(logits = mapOf("SEARCH" to 9.0))
        val resolution = VoiceController(client).resolve("search for 90s shoegaze")
        assertEquals("search for 90s shoegaze", resolution.resolved.transcript)
    }

    @Test
    fun `an inference failure surfaces as a controller error`() {
        val controller = VoiceController(FakeNluInference(failWithCall = true))
        assertThrows<VoiceController.InferenceFailed> {
            runBlocking { controller.resolve("play some jazz by miles davis") }
        }
    }

    @Test
    fun `disabling the fast path routes bare transport through the model`() = runBlocking {
        val client = FakeNluInference(logits = mapOf("PAUSE" to 9.0))
        val controller = VoiceController(client, VoiceController.Config(useFastPath = false))
        val resolution = controller.resolve("pause")
        assertEquals(NluStage.Model, resolution.stage)
        assertEquals("PAUSE", resolution.resolved.intent)
    }
}
