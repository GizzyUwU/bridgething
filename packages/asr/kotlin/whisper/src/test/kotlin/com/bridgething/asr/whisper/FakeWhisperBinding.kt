package com.bridgething.asr.whisper

import java.util.concurrent.atomic.AtomicInteger

class FakeWhisperBinding(
  private val initResult: Long = 42L,
  private val fullStatus: Int = 0,
  private val texts: List<String> = listOf(" hello", " world"),
  private val decodeMillis: Long = 0,
) : WhisperBinding {
  val initCalls = AtomicInteger()
  val releaseCalls = AtomicInteger()
  val maxConcurrentDecodes = AtomicInteger()

  private val activeDecodes = AtomicInteger()

  var lastThreads: Int = 0
    private set

  var lastLanguage: String = ""
    private set

  var lastSamples: FloatArray = FloatArray(0)
    private set

  override fun init(modelPath: String): Long {
    initCalls.incrementAndGet()
    return initResult
  }

  override fun release(handle: Long) {
    releaseCalls.incrementAndGet()
  }

  override fun full(handle: Long, samples: FloatArray, threads: Int, language: String): Int {
    val active = activeDecodes.incrementAndGet()
    maxConcurrentDecodes.getAndUpdate { maxOf(it, active) }
    try {
      lastThreads = threads
      lastLanguage = language
      lastSamples = samples
      if (decodeMillis > 0) Thread.sleep(decodeMillis)
      return fullStatus
    } finally {
      activeDecodes.decrementAndGet()
    }
  }

  override fun segmentCount(handle: Long): Int = texts.size

  override fun segmentText(handle: Long, index: Int): String = texts[index]

  override fun segmentStartMs(handle: Long, index: Int): Long = index * 1000L

  override fun segmentEndMs(handle: Long, index: Int): Long = (index + 1) * 1000L

  override fun segmentConfidence(handle: Long, index: Int): Float = 0.5f + index * 0.25f

  override fun systemInfo(): String = "fake"
}
