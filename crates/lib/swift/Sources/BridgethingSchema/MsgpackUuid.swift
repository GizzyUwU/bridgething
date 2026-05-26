import Foundation

/// Codable property wrapper that bridges `Foundation.UUID` and the 16-byte msgpack `bin` shape.
/// UUID's stock Codable conformance encodes as a hyphenated string (`str`), which breaks
/// the daemon-side serde-msgpack representation; this wrapper encodes as `Data` (`bin`) instead.
@propertyWrapper
public struct MsgpackUuid: Codable, Sendable, Hashable {
  public var wrappedValue: UUID

  public init(wrappedValue: UUID) {
    self.wrappedValue = wrappedValue
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    let data = try container.decode(Data.self)
    guard data.count == 16 else {
      throw DecodingError.dataCorruptedError(
        in: container,
        debugDescription: "expected 16 bytes for a UUID, got \(data.count)"
      )
    }
    let bytes = [UInt8](data)
    let tuple: uuid_t = (
      bytes[0], bytes[1], bytes[2], bytes[3],
      bytes[4], bytes[5], bytes[6], bytes[7],
      bytes[8], bytes[9], bytes[10], bytes[11],
      bytes[12], bytes[13], bytes[14], bytes[15]
    )
    wrappedValue = UUID(uuid: tuple)
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(withUnsafeBytes(of: wrappedValue.uuid) { Data($0) })
  }
}

/// Optional variant of `MsgpackUuid`. Property-wrapper synthesis treats `nil` separately,
/// so the non-optional wrapper cannot serve both roles.
@propertyWrapper
public struct OptionalMsgpackUuid: Codable, Sendable, Hashable {
  public var wrappedValue: UUID?

  public init(wrappedValue: UUID?) {
    self.wrappedValue = wrappedValue
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      wrappedValue = nil
      return
    }
    let data = try container.decode(Data.self)
    guard data.count == 16 else {
      throw DecodingError.dataCorruptedError(
        in: container,
        debugDescription: "expected 16 bytes for an optional UUID, got \(data.count)"
      )
    }
    let bytes = [UInt8](data)
    let tuple: uuid_t = (
      bytes[0], bytes[1], bytes[2], bytes[3],
      bytes[4], bytes[5], bytes[6], bytes[7],
      bytes[8], bytes[9], bytes[10], bytes[11],
      bytes[12], bytes[13], bytes[14], bytes[15]
    )
    wrappedValue = UUID(uuid: tuple)
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    if let value = wrappedValue {
      try container.encode(withUnsafeBytes(of: value.uuid) { Data($0) })
    } else {
      try container.encodeNil()
    }
  }
}
