package com.bridgething.asr.whisper

interface WhisperBinding {
  fun init(modelPath: String): Long

  fun release(handle: Long)

  fun full(handle: Long, samples: FloatArray, threads: Int, language: String): Int

  fun segmentCount(handle: Long): Int

  fun segmentText(handle: Long, index: Int): String

  fun segmentStartMs(handle: Long, index: Int): Long

  fun segmentEndMs(handle: Long, index: Int): Long

  fun segmentConfidence(handle: Long, index: Int): Float

  fun systemInfo(): String
}

object NativeWhisperBinding : WhisperBinding {
  init {
    System.loadLibrary("bridgething_whisper")
  }

  override fun init(modelPath: String): Long = nativeInit(modelPath)

  override fun release(handle: Long) = nativeRelease(handle)

  override fun full(handle: Long, samples: FloatArray, threads: Int, language: String): Int =
    nativeFull(handle, samples, threads, language)

  override fun segmentCount(handle: Long): Int = nativeSegmentCount(handle)

  override fun segmentText(handle: Long, index: Int): String = nativeSegmentText(handle, index)

  override fun segmentStartMs(handle: Long, index: Int): Long = nativeSegmentStartMs(handle, index)

  override fun segmentEndMs(handle: Long, index: Int): Long = nativeSegmentEndMs(handle, index)

  override fun segmentConfidence(handle: Long, index: Int): Float = nativeSegmentConfidence(handle, index)

  override fun systemInfo(): String = nativeSystemInfo()

  private external fun nativeInit(modelPath: String): Long

  private external fun nativeRelease(handle: Long)

  private external fun nativeFull(handle: Long, samples: FloatArray, threads: Int, language: String): Int

  private external fun nativeSegmentCount(handle: Long): Int

  private external fun nativeSegmentText(handle: Long, index: Int): String

  private external fun nativeSegmentStartMs(handle: Long, index: Int): Long

  private external fun nativeSegmentEndMs(handle: Long, index: Int): Long

  private external fun nativeSegmentConfidence(handle: Long, index: Int): Float

  private external fun nativeSystemInfo(): String
}
