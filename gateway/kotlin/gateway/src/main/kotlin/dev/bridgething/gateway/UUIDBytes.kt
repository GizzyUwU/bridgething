package dev.bridgething.gateway

import java.nio.ByteBuffer
import java.util.UUID

/**
 * UUIDs travel on the wire as 16-byte msgpack `bin`. The schema-generated types
 * expose those fields as `ByteArray`; these helpers translate to and from
 * `java.util.UUID` at field boundaries.
 */

public fun uuidFromBytes(bytes: ByteArray): UUID {
  require(bytes.size == 16) { "expected 16 bytes for a UUID, got ${bytes.size}" }
  val buf = ByteBuffer.wrap(bytes)
  val msb = buf.long
  val lsb = buf.long
  return UUID(msb, lsb)
}

public fun UUID.toBytes(): ByteArray {
  val buf = ByteBuffer.allocate(16)
  buf.putLong(mostSignificantBits)
  buf.putLong(leastSignificantBits)
  return buf.array()
}
