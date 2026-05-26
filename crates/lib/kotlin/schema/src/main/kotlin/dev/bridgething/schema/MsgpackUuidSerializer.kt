package dev.bridgething.schema

import java.nio.ByteBuffer
import java.util.UUID
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ByteArraySerializer
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/**
 * UUIDs travel as 16-byte msgpack `bin` on the gateway wire and as
 * hyphenated strings on JSON. This per-field serializer is what
 * codegen attaches to every UUID-typed property in the generated
 * schema; nothing here changes the JVM-wide `java.util.UUID` shape.
 */
public object MsgpackUuidSerializer : KSerializer<UUID> {
  // kotlinx-msgpack pattern-matches on `ByteArraySerializer()` inside its
  // encode/decodeSerializableValue to route through `bin` instead of the generic
  // LIST path; going through encodeSerializableValue(delegate, bytes) triggers that branch.
  private val delegate = ByteArraySerializer()
  override val descriptor: SerialDescriptor = delegate.descriptor

  override fun serialize(encoder: Encoder, value: UUID) {
    val buf = ByteBuffer.allocate(16)
    buf.putLong(value.mostSignificantBits)
    buf.putLong(value.leastSignificantBits)
    encoder.encodeSerializableValue(delegate, buf.array())
  }

  override fun deserialize(decoder: Decoder): UUID {
    val bytes = decoder.decodeSerializableValue(delegate)
    require(bytes.size == 16) { "expected 16-byte UUID, got ${bytes.size} bytes" }
    val buf = ByteBuffer.wrap(bytes)
    return UUID(buf.long, buf.long)
  }
}
