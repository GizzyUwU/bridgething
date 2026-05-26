package dev.bridgething.schema

import kotlinx.serialization.InternalSerializationApi
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerialName
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.SerialKind
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.descriptors.buildSerialDescriptor
import kotlinx.serialization.encoding.CompositeDecoder
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.serializer
import kotlin.reflect.KClass
import kotlin.reflect.full.createInstance
import kotlin.reflect.full.findAnnotation
import kotlin.reflect.full.memberProperties
import kotlin.reflect.full.primaryConstructor

/**
 * Serializer for sealed classes that ride bridgething's wire as adjacently-tagged
 * objects: `{"<discriminator>": "<tag>", "data": <payload>}` (or `{"<discriminator>": "<tag>"}`
 * for unit variants).
 *
 * kotlinx-serialization's default polymorphism for non-JSON formats inlines the
 * tag and content as siblings of the parent map's other keys, which is incompatible
 * with the wire shape Rust serde produces. JSON has `classDiscriminator` config to
 * change this, but msgpack and other binary formats don't expose an equivalent.
 *
 * Each variant is discovered via `kotlin-reflect` from the sealed-class hierarchy,
 * keyed by its `@SerialName`. Data-class variants must have a single property named
 * `data` carrying the variant payload (which is the shape typeshare emits for
 * Rust's `tag = "...", content = "data"` enums).
 */
@OptIn(InternalSerializationApi::class)
public class AdjacentTaggedSerializer<T : Any>(
  private val baseClass: KClass<T>,
  private val discriminator: String,
) : KSerializer<T> {
  private class VariantInfo(
    val tag: String,
    val klass: KClass<*>,
    val payloadSerializer: KSerializer<Any>?,
  )

  private val variantsByTag: Map<String, VariantInfo> by lazy { discoverVariants() }
  private val variantsByClass: Map<KClass<*>, VariantInfo> by lazy {
    variantsByTag.values.associateBy { it.klass }
  }

  @Suppress("UNCHECKED_CAST")
  private fun discoverVariants(): Map<String, VariantInfo> {
    val out = LinkedHashMap<String, VariantInfo>()
    for (sub in baseClass.sealedSubclasses) {
      val tag = sub.findAnnotation<SerialName>()?.value
        ?: error("AdjacentTaggedSerializer: variant ${sub.simpleName} of ${baseClass.simpleName} is missing @SerialName")

      val payloadSerializer: KSerializer<Any>? = if (sub.objectInstance != null) {
        null
      } else {
        val dataProp = sub.memberProperties.firstOrNull { it.name == "data" }
          ?: error("AdjacentTaggedSerializer: variant ${sub.simpleName} of ${baseClass.simpleName} must be `object` or `data class X(val data: ...)`")
        // honor a field-level @Serializable(with = ...) override on the `data`
        // property if present, else fall back to the type's default serializer.
        val withClass = dataProp.findAnnotation<Serializable>()?.with
        if (withClass != null && withClass != KSerializer::class) {
          (withClass.objectInstance ?: withClass.createInstance()) as KSerializer<Any>
        } else {
          serializer(dataProp.returnType) as KSerializer<Any>
        }
      }

      out[tag] = VariantInfo(tag, sub, payloadSerializer)
    }
    require(out.isNotEmpty()) { "AdjacentTaggedSerializer: ${baseClass.simpleName} has no sealed subclasses" }
    return out
  }

  override val descriptor: SerialDescriptor =
    buildClassSerialDescriptor(baseClass.qualifiedName ?: "AdjacentTagged") {
      element(discriminator, String.serializer().descriptor)
      element("data", buildSerialDescriptor("Any", SerialKind.CONTEXTUAL), isOptional = true)
    }

  override fun serialize(encoder: Encoder, value: T) {
    val info = variantsByClass[value::class]
      ?: error("AdjacentTaggedSerializer: no variant info for ${value::class} (not a sealed subclass of ${baseClass.simpleName}?)")

    val composite = encoder.beginStructure(descriptor)
    composite.encodeStringElement(descriptor, 0, info.tag)
    if (info.payloadSerializer != null) {
      val dataProp = info.klass.memberProperties.first { it.name == "data" }
      val payload = dataProp.getter.call(value)
        ?: error("AdjacentTaggedSerializer: variant ${info.klass.simpleName}.data was null")
      composite.encodeSerializableElement(descriptor, 1, info.payloadSerializer, payload)
    }
    composite.endStructure(descriptor)
  }

  override fun deserialize(decoder: Decoder): T {
    val composite = decoder.beginStructure(descriptor)
    var info: VariantInfo? = null
    var payload: Any? = null

    loop@ while (true) {
      when (val idx = composite.decodeElementIndex(descriptor)) {
        0 -> {
          val tag = composite.decodeStringElement(descriptor, 0)
          info = variantsByTag[tag]
            ?: error("AdjacentTaggedSerializer: unknown tag '$tag' for ${baseClass.simpleName}")
          // for unit variants, kotlinx-serialization-msgpack iterates descriptor elements
          // sequentially regardless of wire content and would surface index 1 next, consuming
          // bytes that belong to the parent map. break early since there is no payload.
          if (info.payloadSerializer == null) break@loop
        }
        1 -> {
          val s = info?.payloadSerializer
            ?: error("AdjacentTaggedSerializer: data element arrived before $discriminator on ${baseClass.simpleName}")
          payload = composite.decodeSerializableElement(descriptor, 1, s)
        }
        CompositeDecoder.DECODE_DONE -> break@loop
        else -> error("AdjacentTaggedSerializer: unexpected element index $idx for ${baseClass.simpleName}")
      }
    }
    composite.endStructure(descriptor)

    val resolved = info ?: error("AdjacentTaggedSerializer: missing $discriminator key on ${baseClass.simpleName}")
    @Suppress("UNCHECKED_CAST")
    return if (resolved.payloadSerializer == null) {
      resolved.klass.objectInstance as T
    } else {
      val ctor = resolved.klass.primaryConstructor
        ?: error("AdjacentTaggedSerializer: variant ${resolved.klass.simpleName} has no primary constructor")
      ctor.call(payload) as T
    }
  }
}
