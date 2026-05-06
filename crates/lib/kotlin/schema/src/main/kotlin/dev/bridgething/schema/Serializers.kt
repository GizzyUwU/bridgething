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

public object ConfigFieldSerializer :
    KSerializer<ConfigField> by AdjacentTaggedSerializer(ConfigField::class, discriminator = "type")

public object BridgeToGatewayAssetMsgSerializer :
    KSerializer<BridgeToGatewayAssetMsg> by AdjacentTaggedSerializer(
        BridgeToGatewayAssetMsg::class,
        discriminator = "event"
    )

public object GatewayToBridgeAssetMsgSerializer :
    KSerializer<GatewayToBridgeAssetMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeAssetMsg::class,
        discriminator = "event"
    )

public object GatewayToBridgeAuthorityMsgSerializer :
    KSerializer<GatewayToBridgeAuthorityMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeAuthorityMsg::class,
        discriminator = "event"
    )

public object AssetRetentionSerializer :
    KSerializer<AssetRetention> by AdjacentTaggedSerializer(AssetRetention::class, discriminator = "type")

public object GatewayToBridgeChromeMsgSerializer :
    KSerializer<GatewayToBridgeChromeMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeChromeMsg::class,
        discriminator = "event"
    )

public object ForwardMessageSerializer :
    KSerializer<ForwardMessage> by AdjacentTaggedSerializer(ForwardMessage::class, discriminator = "encoding")

public object BridgeToGatewayTransportMsgSerializer :
    KSerializer<BridgeToGatewayTransportMsg> by AdjacentTaggedSerializer(
        BridgeToGatewayTransportMsg::class,
        discriminator = "event"
    )

public object BridgeToGatewayWebappMsgSerializer :
    KSerializer<BridgeToGatewayWebappMsg> by AdjacentTaggedSerializer(
        BridgeToGatewayWebappMsg::class,
        discriminator = "event"
    )

public object GatewayToBridgeWebappMsgSerializer :
    KSerializer<GatewayToBridgeWebappMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeWebappMsg::class,
        discriminator = "event"
    )

public object BridgeToGatewayVoiceMsgSerializer :
    KSerializer<BridgeToGatewayVoiceMsg> by AdjacentTaggedSerializer(
        BridgeToGatewayVoiceMsg::class,
        discriminator = "event"
    )

public object GatewayToBridgeVoiceMsgSerializer :
    KSerializer<GatewayToBridgeVoiceMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeVoiceMsg::class,
        discriminator = "event"
    )

public object BridgeToGatewayLyricsMsgSerializer :
    KSerializer<BridgeToGatewayLyricsMsg> by AdjacentTaggedSerializer(
        BridgeToGatewayLyricsMsg::class,
        discriminator = "event"
    )

public object GatewayToBridgeLyricsMsgSerializer :
    KSerializer<GatewayToBridgeLyricsMsg> by AdjacentTaggedSerializer(
        GatewayToBridgeLyricsMsg::class,
        discriminator = "event"
    )

public object WireErrorSerializer :
    KSerializer<WireError> by AdjacentTaggedSerializer(WireError::class, discriminator = "type")

public object WebappErrorSerializer :
    KSerializer<WebappError> by AdjacentTaggedSerializer(WebappError::class, discriminator = "type")

public object PeerCompanionStatusSerializer :
    KSerializer<PeerCompanionStatus> by AdjacentTaggedSerializer(
        PeerCompanionStatus::class,
        discriminator = "type"
    )

public object ServerPeerEventSerializer :
    KSerializer<ServerPeerEvent> by AdjacentTaggedSerializer(ServerPeerEvent::class, discriminator = "event")
