package dev.bridgething.schema

import com.ensarsarajcic.kotlinx.serialization.msgpack.MsgPackNullableDynamicSerializer
import kotlinx.serialization.InternalSerializationApi
import kotlinx.serialization.KSerializer
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.SerialKind
import kotlinx.serialization.descriptors.buildSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

/**
 * Serializer for `kotlinx.serialization.json.JsonElement` that works across
 * formats. The schema's `Value = JsonElement` typealias rides the
 * `ForwardMessage.Json` variant — Rust's `serde_json::Value` is format-agnostic
 * (becomes nested JSON in JSON output and nested msgpack maps/arrays/primitives
 * in msgpack output), but kotlinx's built-in `JsonElementSerializer` only works
 * with `JsonEncoder` / `JsonDecoder`.
 *
 * Dispatch:
 *  - JSON formats: delegate to the encoder/decoder's `encodeJsonElement` /
 *    `decodeJsonElement` for native pass-through.
 *  - msgpack (or any other format): translate to/from `Any?` and delegate to
 *    `MsgPackNullableDynamicSerializer`, which peeks msgpack tokens and
 *    dispatches per type.
 *
 * Apply via `@Contextual` at use sites so the runtime serializer lookup picks
 * this up instead of the default `JsonElementSerializer`. The schema's Justfile
 * post-process adds `@Contextual` on the `Forward.Json.data` field.
 */
public object UniversalValueSerializer : KSerializer<JsonElement> {
  @OptIn(InternalSerializationApi::class)
  override val descriptor: SerialDescriptor =
    buildSerialDescriptor("dev.bridgething.schema.Value", SerialKind.CONTEXTUAL)

  override fun serialize(encoder: Encoder, value: JsonElement) {
    if (encoder is JsonEncoder) {
      encoder.encodeJsonElement(value)
      return
    }
    MsgPackNullableDynamicSerializer.Default.serialize(encoder, jsonElementToDynamic(value))
  }

  override fun deserialize(decoder: Decoder): JsonElement {
    if (decoder is JsonDecoder) return decoder.decodeJsonElement()
    val dynamic = MsgPackNullableDynamicSerializer.Default.deserialize(decoder)
    return dynamicToJsonElement(dynamic)
  }

  private fun jsonElementToDynamic(elem: JsonElement): Any? = when (elem) {
    is JsonNull -> null
    is JsonPrimitive -> when {
      elem.isString -> elem.content
      else -> elem.booleanOrNull
        ?: elem.longOrNull
        ?: elem.doubleOrNull
        ?: error("UniversalValueSerializer: unsupported JsonPrimitive '${elem.content}'")
    }
    is JsonObject -> elem.mapValues { jsonElementToDynamic(it.value) }
    is JsonArray -> elem.map { jsonElementToDynamic(it) }
  }

  private fun dynamicToJsonElement(value: Any?): JsonElement = when (value) {
    null -> JsonNull
    is Boolean -> JsonPrimitive(value)
    is Number -> JsonPrimitive(value)
    is String -> JsonPrimitive(value)
    is Map<*, *> -> JsonObject(
      value.entries.associate { (k, v) ->
        (k?.toString() ?: error("UniversalValueSerializer: null map key in msgpack payload")) to dynamicToJsonElement(v)
      }
    )
    is List<*> -> JsonArray(value.map { dynamicToJsonElement(it) })
    is Array<*> -> JsonArray(value.map { dynamicToJsonElement(it) })
    is ByteArray -> JsonArray(value.map { JsonPrimitive(it.toInt() and 0xff) })
    else -> error("UniversalValueSerializer: unsupported dynamic value type ${value::class.qualifiedName}")
  }
}
