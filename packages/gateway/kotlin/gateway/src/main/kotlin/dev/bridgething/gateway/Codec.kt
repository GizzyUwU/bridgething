package dev.bridgething.gateway

import com.ensarsarajcic.kotlinx.serialization.msgpack.MsgPack
import dev.bridgething.schema.Priority
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.SerializationStrategy
import kotlinx.serialization.json.Json
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.util.zip.Deflater
import java.util.zip.GZIPInputStream
import java.util.zip.GZIPOutputStream

public fun Priority.toWireByte(): Byte =
  when (this) {
    Priority.Normal -> 0x00
    Priority.Bulk -> 0x01
    Priority.Background -> 0x02
  }

public fun priorityFromWireByte(b: Byte): Priority =
  when (b) {
    0x01.toByte() -> Priority.Bulk
    0x02.toByte() -> Priority.Background
    else -> Priority.Normal
  }

public enum class Compression(public val byte: Byte) {
  NONE(0x00),
  GZIP(0x01);

  public companion object {
    public fun fromByte(b: Byte): Compression =
      entries.firstOrNull { it.byte == b }
        ?: throw CodecException.UnsupportedCompression(b)
  }
}

public enum class Encoding(public val byte: Byte) {
  MSGPACK(0x00),
  JSON(0x01);

  public companion object {
    public fun fromByte(b: Byte): Encoding =
      entries.firstOrNull { it.byte == b }
        ?: throw CodecException.UnsupportedEncoding(b)
  }
}

public sealed class CodecException(message: String) : RuntimeException(message) {
  public class HeaderTooShort(have: Int) : CodecException("header too short: have $have, need ${FrameHeader.LENGTH}")
  public class InvalidMagic(magic: Int) : CodecException("invalid magic 0x${magic.toString(16)}, expected 0x${FrameHeader.MAGIC.toString(16)}")
  public class UnsupportedVersion(v: Byte) : CodecException("unsupported wire version $v, expected ${FrameHeader.VERSION}")
  public class UnsupportedCompression(b: Byte) : CodecException("unknown compression byte 0x${(b.toInt() and 0xff).toString(16)}")
  public class UnsupportedEncoding(b: Byte) : CodecException("unknown encoding byte 0x${(b.toInt() and 0xff).toString(16)}")
  public class PayloadTooShort(have: Int, need: Int) : CodecException("payload too short: have $have, need $need")
}

/**
 * 16-byte wire header.
 *
 * `| magic u16 BE | version u8 | compression u8 | encoding u8 | priority u8 | reserved [2]u8 | length u64 BE |`
 */
public data class FrameHeader(
  val compression: Compression,
  val encoding: Encoding,
  val priority: Priority,
  val payloadLength: Long,
) {
  public companion object {
    public const val LENGTH: Int = 16
    public const val MAGIC: Int = 0xdead
    public const val VERSION: Byte = 2

    public fun parse(frame: ByteArray): FrameHeader {
      if (frame.size < LENGTH) throw CodecException.HeaderTooShort(frame.size)
      val buf = ByteBuffer.wrap(frame, 0, LENGTH)
      val magic = buf.short.toInt() and 0xffff
      if (magic != MAGIC) throw CodecException.InvalidMagic(magic)
      val ver = buf.get()
      if (ver != VERSION) throw CodecException.UnsupportedVersion(ver)
      val compression = Compression.fromByte(buf.get())
      val encoding = Encoding.fromByte(buf.get())
      val priority = priorityFromWireByte(buf.get())
      buf.position(buf.position() + 2) // reserved
      val length = buf.long
      return FrameHeader(compression, encoding, priority, length)
    }
  }

  public fun write(): ByteArray {
    val buf = ByteBuffer.allocate(LENGTH)
    buf.putShort(MAGIC.toShort())
    buf.put(VERSION)
    buf.put(compression.byte)
    buf.put(encoding.byte)
    buf.put(priority.toWireByte())
    buf.put(byteArrayOf(0, 0)) // reserved
    buf.putLong(payloadLength)
    return buf.array()
  }
}

/**
 * Frames and unframes bridgething wire messages over a transport (BLE, RFCOMM).
 *
 * Encode: `T` -> msgpack/json (encoding) -> gzip/raw (compression) -> 16-byte header + body.
 * Decode: header -> body -> gunzip/raw -> msgpack/json -> `T`.
 *
 * UUID fields on the wire are 16-byte msgpack `bin`. The schema-generated
 * types expose them as `java.util.UUID`; `MsgpackUuidSerializer` in the
 * schema package bridges the bin shape via `@Serializable(with = ...)`.
 */
public class Codec(
  public val defaultCompression: Compression = Compression.NONE,
  public val defaultEncoding: Encoding = Encoding.MSGPACK,
) {
  private val msgpack = MsgPack.Default
  private val json = Json { ignoreUnknownKeys = true }

  public companion object {
    public const val AUTO_GZIP_PAYLOAD_THRESHOLD: Int = 16 * 1024 - 128
  }

  public fun <T> encode(
    serializer: SerializationStrategy<T>,
    message: T,
    priority: Priority = Priority.Normal,
    compression: Compression? = null,
    encoding: Encoding = defaultEncoding,
  ): ByteArray {
    val payload = when (encoding) {
      Encoding.MSGPACK -> msgpack.encodeToByteArray(serializer, message)
      Encoding.JSON -> json.encodeToString(serializer, message).encodeToByteArray()
    }
    var comp = compression ?: defaultCompression
    var body = when (comp) {
      Compression.NONE -> payload
      Compression.GZIP -> gzip(payload)
    }
    if (compression == null && comp == Compression.NONE && priority == Priority.Normal &&
      payload.size > AUTO_GZIP_PAYLOAD_THRESHOLD
    ) {
      val gzipped = gzip(payload)
      if (gzipped.size < payload.size) {
        comp = Compression.GZIP
        body = gzipped
      }
    }
    val header = FrameHeader(comp, encoding, priority, body.size.toLong())
    return header.write() + body
  }

  public fun <T> decode(
    deserializer: DeserializationStrategy<T>,
    frame: ByteArray,
  ): T {
    val header = FrameHeader.parse(frame)
    val total = FrameHeader.LENGTH + header.payloadLength.toInt()
    if (frame.size < total) throw CodecException.PayloadTooShort(frame.size, total)
    val body = frame.copyOfRange(FrameHeader.LENGTH, total)
    val payload = when (header.compression) {
      Compression.NONE -> body
      Compression.GZIP -> gunzip(body)
    }
    return when (header.encoding) {
      Encoding.MSGPACK -> msgpack.decodeFromByteArray(deserializer, payload)
      Encoding.JSON -> json.decodeFromString(deserializer, payload.decodeToString())
    }
  }

  private fun gzip(input: ByteArray): ByteArray {
    val out = ByteArrayOutputStream(input.size)
    object : GZIPOutputStream(out) { init { def.setLevel(Deflater.BEST_SPEED) } }.use { it.write(input) }
    return out.toByteArray()
  }

  private fun gunzip(input: ByteArray): ByteArray =
    GZIPInputStream(ByteArrayInputStream(input)).use { it.readBytes() }
}
