package com.bridgething.asr.whisper

import android.os.Debug
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlin.system.measureTimeMillis
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class WhisperRecognizerInstrumentedTest {
  private val tag = "bridgething-whisper-test"

  private fun stagedFile(argument: String): File? {
    val path = InstrumentationRegistry.getArguments().getString(argument) ?: return null
    val file = File(path)
    return if (file.canRead()) file else null
  }

  private fun pssMb(): Double {
    val info = Debug.MemoryInfo()
    Debug.getMemoryInfo(info)
    return info.totalPss / 1024.0
  }

  @Test
  fun transcribesSpokenFixture() {
    val model = stagedFile("whisperModel")
    val fixture = stagedFile("whisperFixture")
    assumeTrue("whisper model and fixture must be staged on the device", model != null && fixture != null)

    val audio = WavFixture.read(fixture!!)
    assertEquals(WHISPER_SAMPLE_RATE, audio.sampleRate)

    val recognizer = WhisperRecognizer(model!!.absolutePath)
    try {
      runBlocking {
        val baselinePss = pssMb()

        val loadMs = measureTimeMillis { recognizer.prepare() }
        val loadedPss = pssMb()

        var transcription: Transcription? = null
        val coldMs = measureTimeMillis {
          transcription = recognizer.transcribeDetailed(audio.samples, audio.sampleRate)
        }
        val decodedPss = pssMb()

        val warmMs = measureTimeMillis { recognizer.transcribeDetailed(audio.samples, audio.sampleRate) }

        val result = transcription!!
        val audioMs = audio.samples.size * 1000L / audio.sampleRate

        Log.i(tag, "system: ${recognizer.systemInfo()}")
        Log.i(tag, "audio: ${audioMs}ms, model load: ${loadMs}ms, cold: ${coldMs}ms, warm: ${warmMs}ms")
        Log.i(tag, "pss baseline: ${baselinePss}mb, loaded: ${loadedPss}mb, decoded: ${decodedPss}mb")
        Log.i(tag, "confidence: ${result.confidence}, segments: ${result.segments.size}")
        Log.i(tag, "transcript: ${result.text}")
        result.segments.forEach { Log.i(tag, "  [${it.startMs}-${it.endMs}] ${it.confidence} ${it.text}") }

        val transcript = result.text.lowercase()
        assertTrue("transcript was empty", transcript.isNotEmpty())
        assertTrue("expected 'country' in: $transcript", transcript.contains("country"))
        assertTrue("expected 'americans' in: $transcript", transcript.contains("americans"))
      }
    } finally {
      runBlocking { recognizer.close() }
    }
  }

  @Test
  fun reportsNativeSystemInfo() {
    val model = stagedFile("whisperModel")
    assumeTrue("whisper model must be staged on the device", model != null)

    val recognizer = WhisperRecognizer(model!!.absolutePath)
    try {
      val info = runBlocking { recognizer.systemInfo() }
      Log.i(tag, "system info: $info")
      assertTrue("system info was empty", info.isNotEmpty())
    } finally {
      runBlocking { recognizer.close() }
    }
  }
}
