import Foundation

/// Per-stream buffer that takes raw byte chunks and yields complete frames.
///
/// Bytes from a stream-oriented transport (RFCOMM, EASession, sockets) arrive
/// without respect to the bridgething frame boundary. `FrameAccumulator` keeps
/// a rolling buffer, validates each header as soon as 16 bytes are available,
/// waits for the full payload, and pops one complete frame at a time.
///
/// Caller-driven: feed bytes with `append`, drain with repeated `nextFrame()`
/// until it returns nil. Not thread-safe; the gateway owns one per device and
/// only calls it from its own actor.
public struct FrameAccumulator {
  /// Hard ceiling for an individual frame's payload. Defends against a hostile
  /// peer claiming a multi-gigabyte payload before any bytes arrive.
  public static let defaultMaxPayloadSize: Int = 8 * 1024 * 1024

  public enum Error: Swift.Error, Equatable {
    case invalidMagic(UInt16)
    case unsupportedVersion(UInt8)
    case unsupportedCompression(UInt8)
    case unsupportedEncoding(UInt8)
    case payloadTooLarge(UInt64, max: Int)
  }

  public let maxPayloadSize: Int
  private var buffer: Data = .init()

  public init(maxPayloadSize: Int = FrameAccumulator.defaultMaxPayloadSize) {
    self.maxPayloadSize = maxPayloadSize
  }

  public mutating func append(_ chunk: Data) {
    buffer.append(chunk)
  }

  /// Pops one complete frame from the head of the buffer if available.
  /// Returns nil when the buffer doesn't yet contain a full header + payload.
  /// Throws on bad magic, unsupported header bytes, or oversized payloads —
  /// the caller is expected to drop the connection in those cases since the
  /// stream has lost framing and there's no safe resync.
  public mutating func nextFrame() throws -> Data? {
    guard buffer.count >= FrameHeader.length else { return nil }

    let header = buffer.withUnsafeBytes { raw -> Result<(UInt64, Compression, Encoding), Error> in
      let b = raw.bindMemory(to: UInt8.self)
      let magic = UInt16(b[0]) << 8 | UInt16(b[1])
      guard magic == FrameHeader.magic else { return .failure(.invalidMagic(magic)) }
      guard b[2] == FrameHeader.version else { return .failure(.unsupportedVersion(b[2])) }
      guard let comp = Compression(rawValue: b[3]) else {
        return .failure(.unsupportedCompression(b[3]))
      }
      guard let enc = Encoding(rawValue: b[4]) else {
        return .failure(.unsupportedEncoding(b[4]))
      }
      var len: UInt64 = 0
      for i in 8 ..< 16 { len = (len << 8) | UInt64(b[i]) }
      return .success((len, comp, enc))
    }

    let (payloadLen, _, _) = try header.get()
    if payloadLen > UInt64(maxPayloadSize) {
      throw Error.payloadTooLarge(payloadLen, max: maxPayloadSize)
    }

    let total = FrameHeader.length + Int(payloadLen)
    guard buffer.count >= total else { return nil }

    let frame = buffer.prefix(total)
    buffer.removeSubrange(buffer.startIndex ..< buffer.startIndex.advanced(by: total))
    return Data(frame)
  }

  public var bufferedByteCount: Int { buffer.count }

  public mutating func reset() { buffer.removeAll(keepingCapacity: true) }
}
