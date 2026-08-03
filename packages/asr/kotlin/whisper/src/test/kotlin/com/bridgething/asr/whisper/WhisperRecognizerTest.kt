package com.bridgething.asr.whisper

import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows

class WhisperRecognizerTest {
  private fun samples(count: Int = 16_000) = FloatArray(count) { 0.0f }

  private fun recognizer(binding: WhisperBinding) = WhisperRecognizer(modelPath = "/fake/model.bin", binding = binding)

  @Test
  fun `rejects audio that is not 16 khz`() {
    val recognizer = recognizer(FakeWhisperBinding())

    assertThrows<IllegalArgumentException> { runBlocking { recognizer.transcribe(samples(), 44_100) } }
  }

  @Test
  fun `rejects an empty sample buffer`() {
    val recognizer = recognizer(FakeWhisperBinding())

    assertThrows<IllegalArgumentException> { runBlocking { recognizer.transcribe(FloatArray(0), WHISPER_SAMPLE_RATE) } }
  }

  @Test
  fun `argument validation runs before the model is loaded`() {
    val binding = FakeWhisperBinding()
    val recognizer = recognizer(binding)

    assertThrows<IllegalArgumentException> { runBlocking { recognizer.transcribe(samples(), 8_000) } }

    assertEquals(0, binding.initCalls.get())
  }

  @Test
  fun `surfaces a model that fails to load`() {
    val recognizer = recognizer(FakeWhisperBinding(initResult = 0L))

    val error = assertThrows<WhisperException> { runBlocking { recognizer.transcribe(samples(), WHISPER_SAMPLE_RATE) } }

    assertTrue(error.message!!.contains("/fake/model.bin"))
  }

  @Test
  fun `surfaces a failed decode`() {
    val recognizer = recognizer(FakeWhisperBinding(fullStatus = 7))

    val error = assertThrows<WhisperException> { runBlocking { recognizer.transcribe(samples(), WHISPER_SAMPLE_RATE) } }

    assertTrue(error.message!!.contains("7"))
  }

  @Test
  fun `joins segment text and reports per segment detail`() = runBlocking {
    val binding = FakeWhisperBinding(texts = listOf(" ask not", "  what your country"))
    val recognizer = recognizer(binding)

    val result = recognizer.transcribeDetailed(samples(), WHISPER_SAMPLE_RATE)

    assertEquals("ask not what your country", result.text)
    assertEquals(listOf("ask not", "what your country"), result.segments.map { it.text })
    assertEquals(listOf(0L, 1000L), result.segments.map { it.startMs })
    assertEquals(0.625f, result.confidence!!, 1e-6f)
  }

  @Test
  fun `loads the model once across repeated calls`() = runBlocking {
    val binding = FakeWhisperBinding()
    val recognizer = recognizer(binding)

    recognizer.prepare()
    recognizer.transcribe(samples(), WHISPER_SAMPLE_RATE)
    recognizer.transcribe(samples(), WHISPER_SAMPLE_RATE)

    assertEquals(1, binding.initCalls.get())
  }

  @Test
  fun `passes the configured threads and language through`() = runBlocking {
    val binding = FakeWhisperBinding()
    val recognizer = WhisperRecognizer("/fake/model.bin", threads = 3, language = "en", binding = binding)

    recognizer.transcribe(samples(), WHISPER_SAMPLE_RATE)

    assertEquals(3, binding.lastThreads)
    assertEquals("en", binding.lastLanguage)
  }

  @Test
  fun `serializes concurrent transcribe calls`() = runBlocking {
    val binding = FakeWhisperBinding(decodeMillis = 25)
    val recognizer = recognizer(binding)

    (1..8).map { async { recognizer.transcribe(samples(), WHISPER_SAMPLE_RATE) } }.awaitAll()

    assertEquals(1, binding.maxConcurrentDecodes.get())
  }

  @Test
  fun `releases the native context once on close`() = runBlocking {
    val binding = FakeWhisperBinding()
    val recognizer = recognizer(binding)

    recognizer.prepare()
    recognizer.close()

    assertEquals(1, binding.releaseCalls.get())
  }

  @Test
  fun `refuses to transcribe after close`() {
    val binding = FakeWhisperBinding()
    val recognizer = recognizer(binding)

    runBlocking {
      recognizer.prepare()
      recognizer.close()
    }

    assertThrows<WhisperException> { runBlocking { recognizer.transcribe(samples(), WHISPER_SAMPLE_RATE) } }
  }

  @Test
  fun `close without a loaded model does not touch the binding`() = runBlocking {
    val binding = FakeWhisperBinding()

    recognizer(binding).close()

    assertEquals(0, binding.releaseCalls.get())
  }
}
