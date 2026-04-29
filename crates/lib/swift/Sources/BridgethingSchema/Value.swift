import Foundation

/// `Value` is the typeshare placeholder for `serde_json::Value` in Rust.
/// It carries the opaque payload of `ForwardMessage.json`, which is the
/// arbitrary-data escape hatch in the bridgething wire protocol.
///
/// Implemented as a JSON-shaped enum that round-trips through Codable.
public enum Value: Codable, Equatable, Sendable {
  case null
  case bool(Bool)
  case int(Int64)
  case double(Double)
  case string(String)
  case array([Value])
  case object([String: Value])

  public init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      self = .null
    } else if let v = try? container.decode(Bool.self) {
      self = .bool(v)
    } else if let v = try? container.decode(Int64.self) {
      self = .int(v)
    } else if let v = try? container.decode(Double.self) {
      self = .double(v)
    } else if let v = try? container.decode(String.self) {
      self = .string(v)
    } else if let v = try? container.decode([Value].self) {
      self = .array(v)
    } else if let v = try? container.decode([String: Value].self) {
      self = .object(v)
    } else {
      throw DecodingError.dataCorruptedError(
        in: container,
        debugDescription: "unrecognized JSON value"
      )
    }
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    switch self {
    case .null: try container.encodeNil()
    case let .bool(v): try container.encode(v)
    case let .int(v): try container.encode(v)
    case let .double(v): try container.encode(v)
    case let .string(v): try container.encode(v)
    case let .array(v): try container.encode(v)
    case let .object(v): try container.encode(v)
    }
  }
}
