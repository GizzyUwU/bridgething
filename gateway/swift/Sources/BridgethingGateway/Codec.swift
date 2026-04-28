import BridgethingSchema
import Foundation
import Gzip
import MessagePack

public enum Compression: UInt8, Sendable {
  case none = 0x00
  case gzip = 0x01
}

public enum Encoding: UInt8, Sendable {
  case msgpack = 0x00
  case json = 0x01
}

public enum CodecError: Error, Equatable {
  case headerTooShort(have: Int)
  case invalidMagic(UInt16)
  case unsupportedVersion(UInt8)
  case unsupportedCompression(UInt8)
  case unsupportedEncoding(UInt8)
  case payloadTooShort(have: Int, need: Int)
}

/// 16-byte wire header.
///
/// `| magic u16 BE | version u8 | compression u8 | encoding u8 | reserved [3]u8 | length u64 BE |`
public struct FrameHeader: Sendable, Equatable {
  public static let magic: UInt16 = 0xDEAD
  public static let version: UInt8 = 2
  public static let length: Int = 16

  public let compression: Compression
  public let encoding: Encoding
  public let payloadLength: UInt64

  public init(compression: Compression, encoding: Encoding, payloadLength: UInt64) {
    self.compression = compression
    self.encoding = encoding
    self.payloadLength = payloadLength
  }

  public static func parse(_ frame: Data) throws -> FrameHeader {
    guard frame.count >= length else {
      throw CodecError.headerTooShort(have: frame.count)
    }
    let bytes = [UInt8](frame.prefix(length))

    let magic = UInt16(bytes[0]) << 8 | UInt16(bytes[1])
    guard magic == Self.magic else { throw CodecError.invalidMagic(magic) }
    guard bytes[2] == Self.version else { throw CodecError.unsupportedVersion(bytes[2]) }
    guard let compression = Compression(rawValue: bytes[3]) else {
      throw CodecError.unsupportedCompression(bytes[3])
    }
    guard let encoding = Encoding(rawValue: bytes[4]) else {
      throw CodecError.unsupportedEncoding(bytes[4])
    }
    // bytes[5..<8] reserved
    var len: UInt64 = 0
    for i in 8 ..< 16 {
      len = (len << 8) | UInt64(bytes[i])
    }

    return FrameHeader(compression: compression, encoding: encoding, payloadLength: len)
  }

  public func write() -> Data {
    var buf = Data(capacity: Self.length)
    buf.append(UInt8((Self.magic >> 8) & 0xFF))
    buf.append(UInt8(Self.magic & 0xFF))
    buf.append(Self.version)
    buf.append(compression.rawValue)
    buf.append(encoding.rawValue)
    buf.append(contentsOf: [0, 0, 0])
    for shift in stride(from: 56, through: 0, by: -8) {
      buf.append(UInt8((payloadLength >> shift) & 0xFF))
    }
    return buf
  }
}

/// Frames and unframes bridgething wire messages over a transport (BLE, RFCOMM, EAAccessory).
///
/// Encode: `T` → msgpack/json (encoding) → gzip/raw (compression) → 16-byte header + body.
/// Decode: header → body → gunzip/raw → msgpack/json → `T`.
///
/// UUID fields on the wire are 16-byte msgpack `bin`. Use `UUID(data:)` /
/// `Data(uuid:)` (provided as extensions on `UUID`) to translate at field
/// boundaries.
public struct Codec: Sendable {
  public let defaultCompression: Compression
  public let defaultEncoding: Encoding

  public init(compression: Compression = .gzip, encoding: Encoding = .msgpack) {
    defaultCompression = compression
    defaultEncoding = encoding
  }

  public func encode(
    _ message: some Encodable,
    compression: Compression? = nil,
    encoding: Encoding? = nil
  ) throws -> Data {
    let comp = compression ?? defaultCompression
    let enc = encoding ?? defaultEncoding

    let payload: Data = switch enc {
    case .msgpack: try MessagePackEncoder().encode(message)
    case .json: try JSONEncoder().encode(message)
    }

    let body: Data = switch comp {
    case .none: payload
    case .gzip: try payload.gzipped()
    }

    let header = FrameHeader(compression: comp, encoding: enc, payloadLength: UInt64(body.count))
    var frame = header.write()
    frame.append(body)
    return frame
  }

  public func decode<T: Decodable>(_: T.Type, from frame: Data) throws -> T {
    let header = try FrameHeader.parse(frame)
    let total = FrameHeader.length + Int(header.payloadLength)
    guard frame.count >= total else {
      throw CodecError.payloadTooShort(have: frame.count, need: total)
    }

    let body = frame.subdata(in: FrameHeader.length ..< total)
    let payload: Data = switch header.compression {
    case .none: body
    case .gzip: try body.gunzipped()
    }

    switch header.encoding {
    case .msgpack: return try MessagePackDecoder().decode(T.self, from: payload)
    case .json: return try JSONDecoder().decode(T.self, from: payload)
    }
  }
}
