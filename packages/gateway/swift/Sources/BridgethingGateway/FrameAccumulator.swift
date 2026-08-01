import Foundation

public struct FrameAccumulator {
  public static let defaultMaxPayloadSize: Int = 8 * 1024 * 1024

  public enum Error: Swift.Error, Equatable {
    case invalidMagic(UInt16)
    case unsupportedVersion(UInt8)
    case unsupportedCompression(UInt8)
    case unsupportedEncoding(UInt8)
    case payloadTooLarge(UInt64, max: Int)
  }

  private static let compactThreshold = 64 * 1024

  public let maxPayloadSize: Int
  private var buffer: Data = .init()
  private var head: Int = 0

  public init(maxPayloadSize: Int = FrameAccumulator.defaultMaxPayloadSize) {
    self.maxPayloadSize = maxPayloadSize
  }

  public mutating func append(_ chunk: Data) {
    compact()
    buffer.append(chunk)
  }

  private mutating func compact() {
    guard head > 0 else { return }
    if head == buffer.count {
      buffer.removeAll(keepingCapacity: true)
      head = 0
    } else if head >= Self.compactThreshold {
      buffer.removeSubrange(buffer.startIndex ..< buffer.index(buffer.startIndex, offsetBy: head))
      head = 0
    }
  }

  public mutating func nextFrame() throws -> Data? {
    guard buffer.count - head >= FrameHeader.length else { return nil }

    let at = head
    let header = buffer.withUnsafeBytes { raw -> Result<(UInt64, Compression, Encoding), Error> in
      let b = raw.bindMemory(to: UInt8.self)
      let magic = UInt16(b[at]) << 8 | UInt16(b[at + 1])
      guard magic == FrameHeader.magic else { return .failure(.invalidMagic(magic)) }
      guard b[at + 2] == FrameHeader.version else { return .failure(.unsupportedVersion(b[at + 2])) }
      guard let comp = Compression(rawValue: b[at + 3]) else {
        return .failure(.unsupportedCompression(b[at + 3]))
      }
      guard let enc = Encoding(rawValue: b[at + 4]) else {
        return .failure(.unsupportedEncoding(b[at + 4]))
      }
      var len: UInt64 = 0
      for i in (at + 8) ..< (at + 16) {
        len = (len << 8) | UInt64(b[i])
      }
      return .success((len, comp, enc))
    }

    let (payloadLen, _, _) = try header.get()
    if payloadLen > UInt64(maxPayloadSize) {
      throw Error.payloadTooLarge(payloadLen, max: maxPayloadSize)
    }

    let total = FrameHeader.length + Int(payloadLen)
    guard buffer.count - head >= total else { return nil }

    let start = buffer.index(buffer.startIndex, offsetBy: head)
    let frame = buffer.subdata(in: start ..< buffer.index(start, offsetBy: total))
    head += total
    compact()
    return frame
  }

  public var bufferedByteCount: Int { buffer.count - head }

  public mutating func reset() {
    buffer.removeAll(keepingCapacity: true)
    head = 0
  }
}
