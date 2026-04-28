package dev.bridgething.gateway

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer

/**
 * Per-stream buffer that takes raw byte chunks and yields complete frames.
 *
 * Bytes from a stream-oriented transport (RFCOMM, BluetoothSocket, sockets)
 * arrive without respect to the bridgething frame boundary. [FrameAccumulator]
 * keeps a rolling buffer, validates each header as soon as 16 bytes are
 * available, waits for the full payload, and pops one complete frame at a
 * time.
 *
 * Caller-driven: feed bytes with [append], drain with repeated [nextFrame]
 * until it returns null. Not thread-safe; the gateway owns one per device and
 * only calls it from inside its own mutex.
 */
public class FrameAccumulator(
  public val maxPayloadSize: Int = DEFAULT_MAX_PAYLOAD_SIZE,
) {
  public companion object {
    /**
     * Hard ceiling for an individual frame's payload. Defends against a
     * hostile peer claiming a multi-gigabyte payload before any bytes
     * actually arrive.
     */
    public const val DEFAULT_MAX_PAYLOAD_SIZE: Int = 8 * 1024 * 1024
  }

  public sealed class Exception(message: String) : RuntimeException(message) {
    public class InvalidMagic(public val magic: Int) :
      Exception("invalid magic 0x${magic.toString(16)}")
    public class UnsupportedVersion(public val version: Byte) :
      Exception("unsupported wire version $version")
    public class UnsupportedCompression(b: Byte) :
      Exception("unknown compression byte 0x${(b.toInt() and 0xff).toString(16)}")
    public class UnsupportedEncoding(b: Byte) :
      Exception("unknown encoding byte 0x${(b.toInt() and 0xff).toString(16)}")
    public class PayloadTooLarge(public val payloadLength: Long, public val max: Int) :
      Exception("payload $payloadLength exceeds max $max")
  }

  private val buffer = ByteArrayOutputStream()

  public fun append(chunk: ByteArray) {
    buffer.write(chunk)
  }

  /**
   * Pops one complete frame from the head of the buffer if available. Returns
   * null when the buffer doesn't yet contain a full header + payload. Throws
   * on bad magic, unsupported header bytes, or oversized payloads — the
   * caller is expected to drop the connection in those cases since the stream
   * has lost framing and there's no safe resync.
   */
  public fun nextFrame(): ByteArray? {
    val bytes = buffer.toByteArray()
    if (bytes.size < FrameHeader.LENGTH) return null

    val header = ByteBuffer.wrap(bytes, 0, FrameHeader.LENGTH)
    val magic = header.short.toInt() and 0xffff
    if (magic != FrameHeader.MAGIC) throw Exception.InvalidMagic(magic)
    val version = header.get()
    if (version != FrameHeader.VERSION) throw Exception.UnsupportedVersion(version)
    val compressionByte = header.get()
    if (Compression.entries.none { it.byte == compressionByte }) {
      throw Exception.UnsupportedCompression(compressionByte)
    }
    val encodingByte = header.get()
    if (Encoding.entries.none { it.byte == encodingByte }) {
      throw Exception.UnsupportedEncoding(encodingByte)
    }
    header.position(header.position() + 3) // reserved
    val payloadLen = header.long
    if (payloadLen > maxPayloadSize) throw Exception.PayloadTooLarge(payloadLen, maxPayloadSize)

    val total = FrameHeader.LENGTH + payloadLen.toInt()
    if (bytes.size < total) return null

    val frame = bytes.copyOfRange(0, total)
    val rest = bytes.copyOfRange(total, bytes.size)
    buffer.reset()
    if (rest.isNotEmpty()) buffer.write(rest)
    return frame
  }

  public val bufferedByteCount: Int get() = buffer.size()

  public fun reset() { buffer.reset() }
}
