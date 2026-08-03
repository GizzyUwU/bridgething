package com.bridgething.asr.whisper

import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

class WavAudio(val samples: FloatArray, val sampleRate: Int, val channels: Int)

object WavFixture {
  fun read(file: File): WavAudio {
    val buffer = ByteBuffer.wrap(file.readBytes()).order(ByteOrder.LITTLE_ENDIAN)

    require(buffer.int == 0x46464952) { "not a RIFF file: ${file.path}" }
    buffer.int
    require(buffer.int == 0x45564157) { "not a WAVE file: ${file.path}" }

    var sampleRate = 0
    var channels = 0
    var bitsPerSample = 0
    var samples: FloatArray? = null

    while (buffer.remaining() >= 8) {
      val id = buffer.int
      val size = buffer.int
      val next = buffer.position() + size + (size and 1)

      when (id) {
        0x20746d66 -> {
          buffer.short
          channels = buffer.short.toInt()
          sampleRate = buffer.int
          buffer.int
          buffer.short
          bitsPerSample = buffer.short.toInt()
        }
        0x61746164 -> {
          require(bitsPerSample == 16) { "only 16-bit pcm is supported, got $bitsPerSample" }
          val frames = size / 2 / channels
          val decoded = FloatArray(frames)
          for (frame in 0 until frames) {
            var total = 0
            for (channel in 0 until channels) {
              total += buffer.short.toInt()
            }
            decoded[frame] = (total.toFloat() / channels) / 32768.0f
          }
          samples = decoded
        }
      }

      buffer.position(minOf(next, buffer.limit()))
    }

    return WavAudio(
      samples = requireNotNull(samples) { "wav has no data chunk: ${file.path}" },
      sampleRate = sampleRate,
      channels = channels,
    )
  }
}
