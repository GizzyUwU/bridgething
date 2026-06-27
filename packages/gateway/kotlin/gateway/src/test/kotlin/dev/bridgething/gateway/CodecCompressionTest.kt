package dev.bridgething.gateway

import dev.bridgething.schema.Priority
import kotlinx.serialization.builtins.serializer
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class CodecCompressionTest {
  private val codec = Codec(defaultCompression = Compression.NONE, defaultEncoding = Encoding.MSGPACK)

  private fun oversizedPayload(): String = "x".repeat(Codec.AUTO_GZIP_PAYLOAD_THRESHOLD + 2048)

  private fun incompressibleData(n: Int): ByteArray {
    var state = 0x9E3779B97F4A7C15uL
    val out = ByteArray(n)
    for (i in 0 until n) {
      state = state xor (state shl 13)
      state = state xor (state shr 7)
      state = state xor (state shl 17)
      out[i] = (state and 0xFFuL).toByte()
    }
    return out
  }

  @Test
  fun `small frame stays uncompressed`() {
    val frame = codec.encode(String.serializer(), "hello")
    assertEquals(Compression.NONE, FrameHeader.parse(frame).compression)
  }

  @Test
  fun `oversized normal frame auto-gzips`() {
    val payload = oversizedPayload()
    val frame = codec.encode(String.serializer(), payload)
    assertEquals(Compression.GZIP, FrameHeader.parse(frame).compression)
    assertTrue(frame.size < payload.toByteArray().size, "gzip actually shrank the frame")
    assertEquals(payload, codec.decode(String.serializer(), frame))
  }

  @Test
  fun `oversized incompressible normal frame stays raw`() {
    val blob = incompressibleData(Codec.AUTO_GZIP_PAYLOAD_THRESHOLD + 4096)
    val frame = codec.encode(ByteArraySerializer, blob)
    assertEquals(Compression.NONE, FrameHeader.parse(frame).compression, "incompressible payload stays raw")
    assertArrayEquals(blob, codec.decode(ByteArraySerializer, frame))
  }

  @Test
  fun `oversized bulk frame stays raw`() {
    val frame = codec.encode(String.serializer(), oversizedPayload(), priority = Priority.Bulk)
    assertEquals(Compression.NONE, FrameHeader.parse(frame).compression)
  }

  @Test
  fun `explicit none overrides auto-gzip`() {
    val frame = codec.encode(String.serializer(), oversizedPayload(), compression = Compression.NONE)
    assertEquals(Compression.NONE, FrameHeader.parse(frame).compression)
  }

  private companion object {
    val ByteArraySerializer = kotlinx.serialization.builtins.ByteArraySerializer()
  }
}
