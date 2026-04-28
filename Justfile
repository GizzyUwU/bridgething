run:
  cargo run -p bridgething

build:
  just typescript
  cargo build
  bun run build

gateway:
  bun run build -- --filter=@bridgething/gateway
  bun run gateway:example:dev

adapter:
  bun run build -- --filter=@bridgething/adapter-node

typescript:
  rm -rf lib/ts/bindings
  cargo test -p libbridgething &> /dev/null
  bunx prettier lib/ts/bindings --write

swift:
  typeshare --lang=swift --output-file=lib/swift/Sources/BridgethingSchema/Generated.swift lib/src/
  # typeshare emits [UInt8] for Vec<u8>, but Swift's Codable + every msgpack lib
  # distinguish Data (encodes to msgpack bin) from [UInt8] (encodes to array of int).
  # Our wire is bin; rewrite the type so consumers get Data.
  sed -i 's/\[UInt8\]/Data/g' lib/swift/Sources/BridgethingSchema/Generated.swift
  # Generated structs/enums travel through actor-isolated stream events on the
  # gateway side, which Swift 6 strict concurrency requires to be Sendable.
  # Every typeshare-emitted type is a value type whose stored fields are
  # already Sendable (primitives, Data, other generated types), so blanket-
  # adding the conformance is safe.
  sed -i 's/: Codable {/: Codable, Sendable {/g' lib/swift/Sources/BridgethingSchema/Generated.swift
  sed -i 's/: String, Codable {/: String, Codable, Sendable {/g' lib/swift/Sources/BridgethingSchema/Generated.swift

kotlin:
  typeshare --lang=kotlin --java-package=dev.bridgething.schema --output-file=lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt lib/src/
  # typeshare emits List<UByte> for Vec<u8>; rewrite to ByteArray so kotlinx-
  # serialization-msgpack encodes binary fields as msgpack bin (not an array).
  sed -i 's/List<UByte>/ByteArray/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  # Override the auto-generated kotlinx serializer for adjacently-tagged sealed
  # classes. kotlinx-serialization's default polymorphism for binary formats
  # sibling-inlines the discriminator + content into the parent map, but our
  # wire shape is a nested {<disc>: "tag", "data": payload} object. The
  # AdjacentTaggedSerializer proxies in Serializers.kt produce that shape.
  perl -i -0pe 's/\@Serializable\nsealed class GatewayMsgMeta\b/\@Serializable(with = GatewayMsgMetaSerializer::class)\nsealed class GatewayMsgMeta/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  perl -i -0pe 's/\@Serializable\nsealed class BridgeToGatewayMsgData\b/\@Serializable(with = BridgeToGatewayMsgDataSerializer::class)\nsealed class BridgeToGatewayMsgData/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  perl -i -0pe 's/\@Serializable\nsealed class GatewayToBridgeMsgData\b/\@Serializable(with = GatewayToBridgeMsgDataSerializer::class)\nsealed class GatewayToBridgeMsgData/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  perl -i -0pe 's/\@Serializable\nsealed class Image\b/\@Serializable(with = ImageSerializer::class)\nsealed class Image/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  perl -i -0pe 's/\@Serializable\nsealed class BridgeToGatewayFileMsg\b/\@Serializable(with = BridgeToGatewayFileMsgSerializer::class)\nsealed class BridgeToGatewayFileMsg/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  perl -i -0pe 's/\@Serializable\nsealed class GatewayToBridgeFileMsg\b/\@Serializable(with = GatewayToBridgeFileMsgSerializer::class)\nsealed class GatewayToBridgeFileMsg/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  perl -i -0pe 's/\@Serializable\nsealed class GatewayToBridgeChromeMsg\b/\@Serializable(with = GatewayToBridgeChromeMsgSerializer::class)\nsealed class GatewayToBridgeChromeMsg/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  perl -i -0pe 's/\@Serializable\nsealed class ForwardMessage\b/\@Serializable(with = ForwardMessageSerializer::class)\nsealed class ForwardMessage/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt
  # Forward.Json carries an opaque payload typed as `Value = JsonElement`.
  # kotlinx's default JsonElement serializer only works with JsonDecoder;
  # UniversalValueSerializer dispatches on encoder/decoder type so the variant
  # round-trips over msgpack too.
  perl -i -pe 's/data class Json\(val data: Value\)/data class Json(\@Serializable(with = UniversalValueSerializer::class) val data: Value)/g' lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt

codegen: typescript swift kotlin

goldens:
  UPDATE_GOLDEN=1 cargo test -p libbridgething --test golden golden_vectors_match_fixture_file

class:
  sudo hciconfig hci0 class 0x7c0000 || true
  sudo hciconfig hci1 class 0x7c0000 || true
  sudo hciconfig hci2 class 0x7c0000 || true

tokei:
  tokei -t Nix,Rust,TypeScript,TSX,JavaScript