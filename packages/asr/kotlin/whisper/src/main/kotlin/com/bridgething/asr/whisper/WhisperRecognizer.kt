package com.bridgething.asr.whisper

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

const val WHISPER_SAMPLE_RATE: Int = 16_000

data class WhisperSegment(val text: String, val startMs: Long, val endMs: Long, val confidence: Float)

data class Transcription(val text: String, val segments: List<WhisperSegment>, val confidence: Float?)

class WhisperException(message: String) : Exception(message)

class WhisperRecognizer(
  private val modelPath: String,
  private val threads: Int = defaultThreads(),
  private val language: String = "en",
  private val binding: WhisperBinding = NativeWhisperBinding,
  private val dispatcher: CoroutineDispatcher = Dispatchers.Default,
) {
  private val lock = Mutex()
  private var handle = 0L
  private var closed = false

  suspend fun prepare() {
    lock.withLock { withContext(dispatcher) { ensureLoaded() } }
  }

  suspend fun transcribeDetailed(samples: FloatArray, sampleRate: Int): Transcription {
    require(sampleRate == WHISPER_SAMPLE_RATE) {
      "whisper requires ${WHISPER_SAMPLE_RATE} hz mono audio, got $sampleRate hz"
    }
    require(samples.isNotEmpty()) { "cannot transcribe an empty sample buffer" }

    return lock.withLock {
      withContext(dispatcher) {
        ensureLoaded()

        val status = binding.full(handle, samples, threads, language)
        if (status != 0) throw WhisperException("whisper_full failed with status $status")

        collect()
      }
    }
  }

  suspend fun systemInfo(): String = lock.withLock { withContext(dispatcher) { binding.systemInfo() } }

  suspend fun close() {
    lock.withLock {
      withContext(dispatcher) {
        if (handle != 0L) binding.release(handle)
        handle = 0L
        closed = true
      }
    }
  }

  private fun ensureLoaded() {
    if (closed) throw WhisperException("recognizer is closed")
    if (handle != 0L) return

    val loaded = binding.init(modelPath)
    if (loaded == 0L) throw WhisperException("failed to load whisper model at $modelPath")
    handle = loaded
  }

  private fun collect(): Transcription {
    val count = binding.segmentCount(handle)
    val raw = (0 until count).map { binding.segmentText(handle, it) }

    val segments =
      (0 until count).map { index ->
        WhisperSegment(
          text = raw[index].trim(),
          startMs = binding.segmentStartMs(handle, index),
          endMs = binding.segmentEndMs(handle, index),
          confidence = binding.segmentConfidence(handle, index),
        )
      }

    val text = raw.joinToString("").trim().replace(WHITESPACE, " ")
    val confidence = segments.map { it.confidence }.average().takeUnless { segments.isEmpty() }

    return Transcription(text = text, segments = segments, confidence = confidence?.toFloat())
  }

  private companion object {
    val WHITESPACE = Regex("\\s+")
  }
}

private fun defaultThreads(): Int = Runtime.getRuntime().availableProcessors().coerceIn(1, 4)
