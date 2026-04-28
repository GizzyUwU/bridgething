package dev.bridgething.schema

import kotlinx.serialization.KSerializer

/**
 * Per-class adjacent-tagged serializers, one per multi-discriminator sealed
 * class in the schema. Generated.kt is post-processed by `just kotlin` to apply
 * `@Serializable(with = ...)` to each affected sealed class so kotlinx-serialization
 * picks these up regardless of format (msgpack, json, ...).
 *
 * Adding a new variant in Rust requires no edit here - `AdjacentTaggedSerializer`
 * uses `kotlin-reflect` to discover sealed subclasses and their `@SerialName` tags
 * at runtime.
 */

public object GatewayMsgMetaSerializer :
    KSerializer<GatewayMsgMeta> by AdjacentTaggedSerializer(GatewayMsgMeta::class, discriminator = "kind")

public object BridgeToGatewayMsgDataSerializer :
    KSerializer<BridgeToGatewayMsgData> by AdjacentTaggedSerializer(
        BridgeToGatewayMsgData::class,
        discriminator = "type"
    )

public object GatewayToBridgeMsgDataSerializer :
    KSerializer<GatewayToBridgeMsgData> by AdjacentTaggedSerializer(
        GatewayToBridgeMsgData::class,
        discriminator = "type"
    )

public object ImageSerializer :
    KSerializer<Image> by AdjacentTaggedSerializer(Image::class, discriminator = "type")

public object BridgeToGatewayFileMsgSerializer :
    KSerializer<BridgeToGatewayFileMsg> by AdjacentTaggedSerializer(
        BridgeToGatewayFileMsg::class,
        discriminator = "event"
    )

public object GatewayToBridgeFileMsgSerializer :
    KSerializer<GatewayToBridgeFileMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeFileMsg::class,
        discriminator = "event"
    )

public object GatewayToBridgeChromeMsgSerializer :
    KSerializer<GatewayToBridgeChromeMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeChromeMsg::class,
        discriminator = "event"
    )

public object ForwardMessageSerializer :
    KSerializer<ForwardMessage> by AdjacentTaggedSerializer(ForwardMessage::class, discriminator = "encoding")
