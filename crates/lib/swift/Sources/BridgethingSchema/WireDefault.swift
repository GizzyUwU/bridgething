import Foundation

public protocol WireDefaultProvider {
  associatedtype Value: Codable & Sendable
  static var wireDefault: Value { get }
}

@propertyWrapper
public struct WireDefault<P: WireDefaultProvider>: Codable, Sendable {
  public var wrappedValue: P.Value

  public init(wrappedValue: P.Value) {
    self.wrappedValue = wrappedValue
  }

  public init(from decoder: Decoder) throws {
    wrappedValue = try P.Value(from: decoder)
  }

  public func encode(to encoder: Encoder) throws {
    try wrappedValue.encode(to: encoder)
  }
}

public extension KeyedDecodingContainer {
  func decode<P>(_ type: WireDefault<P>.Type, forKey key: Key) throws -> WireDefault<P> {
    guard let decoded = try decodeIfPresent(type, forKey: key) else {
      return WireDefault(wrappedValue: P.wireDefault)
    }
    return decoded
  }
}
