import Foundation

/// Codable property wrapper that bridges `Foundation.UUID` and the
/// 16-byte msgpack `bin` shape on the gateway wire. UUID's stock
/// Codable conformance rides as a hyphenated string, which would land
/// as msgpack `str` and break the daemon-side serde-msgpack
/// representation; this wrapper rides as `Data` instead.
///
/// JSON encoders see Codable's standard Data shape (base64 string).
/// The bridgething JSON path goes through the local websocket — and
/// the daemon's serde-json emits hyphenated UUID strings there — so
/// this wrapper is gateway-only by construction (the JSON path doesn't
/// flow through schema structs that use it).
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
    self.wrappedValue = UUID(uuid: tuple)
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(withUnsafeBytes(of: wrappedValue.uuid) { Data($0) })
  }
}

/// Optional sibling of `MsgpackUuid` for Rust `Option<Uuid>` fields.
/// Codable property-wrapper synthesis treats `nil` separately so the
/// non-optional wrapper can't double-duty.
@propertyWrapper
public struct OptionalMsgpackUuid: Codable, Sendable, Hashable {
  public var wrappedValue: UUID?

  public init(wrappedValue: UUID?) {
    self.wrappedValue = wrappedValue
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      self.wrappedValue = nil
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
    self.wrappedValue = UUID(uuid: tuple)
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
