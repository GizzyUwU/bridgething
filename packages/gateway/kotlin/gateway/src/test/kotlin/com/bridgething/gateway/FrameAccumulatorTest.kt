package com.bridgething.gateway

import com.bridgething.schema.BridgeToGatewayMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.MsgMeta
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Test
import java.util.UUID

class FrameAccumulatorTest {
  private val codec = Codec(defaultCompression = Compression.NONE, defaultEncoding = Encoding.MSGPACK)

  @Test
  fun `nil when empty`() {
    val acc = FrameAccumulator()
    assertNull(acc.nextFrame())
  }

  @Test
  fun `nil when header incomplete`() {
    val acc = FrameAccumulator()
    acc.append(byteArrayOf(0xDE.toByte(), 0xAD.toByte(), 0x02, 0x00, 0x00))
    assertNull(acc.nextFrame())
  }

  @Test
  fun `nil when payload incomplete`() {
    val frame = makeAckFrame()
    val acc = FrameAccumulator()
    acc.append(frame.copyOfRange(0, frame.size - 1))
    assertNull(acc.nextFrame())
  }

  @Test
  fun `emits complete frame`() {
    val frame = makeAckFrame()
    val acc = FrameAccumulator()
    acc.append(frame)
    assertArrayEquals(frame, acc.nextFrame())
    assertNull(acc.nextFrame())
  }

  @Test
  fun `emits multiple frames in single chunk`() {
    val f1 = makeAckFrame()
    val f2 = makeAckFrame()
    val acc = FrameAccumulator()
    acc.append(f1 + f2)
    assertArrayEquals(f1, acc.nextFrame())
    assertArrayEquals(f2, acc.nextFrame())
    assertNull(acc.nextFrame())
  }

  @Test
  fun `reassembles byte at a time`() {
    val frame = makeAckFrame()
    val acc = FrameAccumulator()
    for (b in frame) acc.append(byteArrayOf(b))
    assertArrayEquals(frame, acc.nextFrame())
  }

  @Test
  fun `keeps trailing partial frame`() {
    val f1 = makeAckFrame()
    val f2 = makeAckFrame()
    val acc = FrameAccumulator()
    acc.append(f1 + f2.copyOfRange(0, 5))
    assertArrayEquals(f1, acc.nextFrame())
    assertNull(acc.nextFrame())
    acc.append(f2.copyOfRange(5, f2.size))
    assertArrayEquals(f2, acc.nextFrame())
  }

  @Test
  fun `reassembles large frame across many chunks`() {
    val big = ByteArray(64 * 1024) { (it and 0xff).toByte() }
    val msg = BridgeToGatewayMsg(
      id = UUID.randomUUID(),
      meta = MsgMeta.Command,
      data = BridgeToGatewayMsgData.Forward(
        com.bridgething.schema.ForwardMessage.Binary(big),
      ),
    )
    val frame = codec.encode(BridgeToGatewayMsg.serializer(), msg)
    val acc = FrameAccumulator()
    var off = 0
    val chunk = 4096
    while (off < frame.size) {
      val end = minOf(off + chunk, frame.size)
      acc.append(frame.copyOfRange(off, end))
      if (end < frame.size) assertNull(acc.nextFrame())
      off = end
    }
    assertArrayEquals(frame, acc.nextFrame())
    assertNull(acc.nextFrame())
  }

  @Test
  fun `drains many sequential frames without unbounded growth`() {
    val acc = FrameAccumulator()
    repeat(1000) {
      val frame = makeAckFrame()
      acc.append(frame)
      assertArrayEquals(frame, acc.nextFrame())
    }
    assertEquals(0, acc.bufferedByteCount)
  }

  @Test
  fun `throws on bad magic`() {
    val acc = FrameAccumulator()
    val bytes = ByteArray(FrameHeader.LENGTH)
    bytes[0] = 0xBA.toByte(); bytes[1] = 0xAD.toByte()
    acc.append(bytes)
    val ex = assertThrows(FrameAccumulator.Exception.InvalidMagic::class.java) { acc.nextFrame() }
    assertEquals(0xBAAD, ex.magic)
  }

  @Test
  fun `throws on unsupported version`() {
    val acc = FrameAccumulator()
    val bytes = ByteArray(FrameHeader.LENGTH)
    bytes[0] = 0xDE.toByte(); bytes[1] = 0xAD.toByte()
    bytes[2] = 99.toByte()
    acc.append(bytes)
    val ex = assertThrows(FrameAccumulator.Exception.UnsupportedVersion::class.java) { acc.nextFrame() }
    assertEquals(99.toByte(), ex.version)
  }

  @Test
  fun `throws on oversized payload`() {
    val acc = FrameAccumulator(maxPayloadSize = 1024)
    val bytes = ByteArray(FrameHeader.LENGTH)
    bytes[0] = 0xDE.toByte(); bytes[1] = 0xAD.toByte()
    bytes[2] = FrameHeader.VERSION
    val big = (1L shl 20)
    for (i in 0 until 8) {
      bytes[8 + i] = ((big shr ((7 - i) * 8)) and 0xff).toByte()
    }
    acc.append(bytes)
    val ex = assertThrows(FrameAccumulator.Exception.PayloadTooLarge::class.java) { acc.nextFrame() }
    assertEquals(big, ex.payloadLength)
    assertEquals(1024, ex.max)
  }

  private fun makeAckFrame(): ByteArray {
    val msg = BridgeToGatewayMsg(
      id = UUID.randomUUID(),
      meta = MsgMeta.Command,
      data = BridgeToGatewayMsgData.Ack,
    )
    return codec.encode(BridgeToGatewayMsg.serializer(), msg)
  }
}
