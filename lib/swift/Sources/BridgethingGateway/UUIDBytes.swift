import Foundation

/// UUIDs travel on the wire as 16-byte msgpack `bin`. The schema-generated
/// types expose those fields as `Data`; these initializers translate to and
/// from the native `UUID` type at field boundaries.
public extension UUID {
  init?(data: Data) {
    guard data.count == 16 else { return nil }
    let bytes = [UInt8](data)
    let tuple: uuid_t = (
      bytes[0], bytes[1], bytes[2], bytes[3],
      bytes[4], bytes[5], bytes[6], bytes[7],
      bytes[8], bytes[9], bytes[10], bytes[11],
      bytes[12], bytes[13], bytes[14], bytes[15]
    )
    self.init(uuid: tuple)
  }

  var data: Data {
    withUnsafeBytes(of: uuid) { Data($0) }
  }
}
