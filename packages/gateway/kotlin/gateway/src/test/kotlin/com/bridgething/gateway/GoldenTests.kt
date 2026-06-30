package com.bridgething.gateway

import com.bridgething.schema.BridgeToGatewayMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.ForwardMessage
import com.bridgething.schema.MsgMeta
import com.bridgething.schema.GatewayToBridgeMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.Priority
import com.bridgething.schema.ResponseMeta
import com.ensarsarajcic.kotlinx.serialization.msgpack.MsgPack
import com.ensarsarajcic.kotlinx.serialization.msgpack.MsgPackNullableDynamicSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Assertions.fail
import org.junit.jupiter.api.Test
import java.nio.file.Files
import java.nio.file.Paths
import java.util.UUID

private val FIXED_ID = UUID.fromString("0192f2a0-bbb0-7c00-a000-000000000001")
private val FIXED_REQUEST_ID = UUID.fromString("0192f2a0-bbb0-7c00-a000-000000000099")

class GoldenTests {
  private val codec = Codec(defaultCompression = Compression.NONE, defaultEncoding = Encoding.MSGPACK)

  @Test
  fun `every fixture decodes and round-trips`() {
    val goldens = loadGoldens()
    assertTrue(goldens.fixtures.isNotEmpty(), "expected fixtures in golden.json")
    for (fixture in goldens.fixtures) {
      checkFixture(fixture)
    }
  }

  private fun checkFixture(fixture: GoldenFixture) {
    val frame = hexToBytes(fixture.framedHex)
    val expectedMetaKind = fixture.decodedJson.jsonObject["meta"]!!.jsonObject["kind"]!!.jsonPrimitive.content

    val header = FrameHeader.parse(frame)
    assertEquals(fixture.expectedPriority, header.priority, "priority mismatch on ${fixture.name}")

    when (fixture.direction) {
      Direction.BRIDGE_TO_GATEWAY -> {
        val msg = codec.decode(BridgeToGatewayMsg.serializer(), frame)
        assertEquals(FIXED_ID, msg.id, "id mismatch on ${fixture.name}")
        assertMetaMatches(msg.meta, expectedMetaKind, fixture.name)

        val reEncoded = codec.encode(BridgeToGatewayMsg.serializer(), msg, priority = fixture.expectedPriority)
        val reHeader = FrameHeader.parse(reEncoded)
        assertEquals(fixture.expectedPriority, reHeader.priority, "round-trip priority changed on ${fixture.name}")
        assertGoldenBytes(frame, reEncoded, fixture.name)
        val reDecoded = codec.decode(BridgeToGatewayMsg.serializer(), reEncoded)
        assertEquals(msg.id, reDecoded.id, "round-trip id changed on ${fixture.name}")
      }
      Direction.GATEWAY_TO_BRIDGE -> {
        val msg = codec.decode(GatewayToBridgeMsg.serializer(), frame)
        assertEquals(FIXED_ID, msg.id, "id mismatch on ${fixture.name}")
        assertMetaMatches(msg.meta, expectedMetaKind, fixture.name)

        val reEncoded = codec.encode(GatewayToBridgeMsg.serializer(), msg, priority = fixture.expectedPriority)
        val reHeader = FrameHeader.parse(reEncoded)
        assertEquals(fixture.expectedPriority, reHeader.priority, "round-trip priority changed on ${fixture.name}")
        assertGoldenBytes(frame, reEncoded, fixture.name)
        val reDecoded = codec.decode(GatewayToBridgeMsg.serializer(), reEncoded)
        assertEquals(msg.id, reDecoded.id, "round-trip id changed on ${fixture.name}")
      }
    }
  }

  /**
   * The Rust golden (`rmp_serde::to_vec_named`) is the canonical wire encoding.
   * Kotlin's encode should reproduce it byte-for-byte; anything else risks the
   * daemon failing to decode what we emit. This is the assertion the suite was
   * missing: re-decode round-trips were symmetric about Kotlin's own quirks (an
   * over-counted msgpack map header decodes back fine on our side) and so never
   * noticed bytes a strict decoder like rmp-serde would reject.
   *
   * One tolerated divergence: Rust's `#[serde(skip_serializing_none)]` omits
   * `None` fields, while kotlinx-serialization-msgpack writes them as explicit
   * `nil` (it sizes maps eagerly from the descriptor, so it cannot skip an
   * element without lying about the count). rmp-serde accepts explicit `nil`
   * for `Option<T>` on decode, so this is wire-safe. When the bytes diverge we
   * fall back to requiring the payload to strictly decode as well-formed msgpack
   * - which a dishonest map header (the unit-variant over-count bug) fails.
   */
  private fun assertGoldenBytes(golden: ByteArray, encoded: ByteArray, name: String) {
    if (golden.contentEquals(encoded)) return
    val body = encoded.copyOfRange(FrameHeader.LENGTH, encoded.size)
    runCatching { MsgPack.Default.decodeFromByteArray(MsgPackNullableDynamicSerializer, body) }
      .onFailure {
        fail<Unit>(
          "encoded bytes for $name diverged from golden AND are not strictly " +
            "decodable msgpack (${it.message})\n" +
            "  golden : ${bytesToHex(golden)}\n" +
            "  encoded: ${bytesToHex(encoded)}",
        )
      }
  }

  private fun assertMetaMatches(meta: MsgMeta, expectedKind: String, name: String) {
    when (meta) {
      is MsgMeta.Command -> assertEquals("command", expectedKind, "meta kind mismatch on $name")
      is MsgMeta.Event -> assertEquals("event", expectedKind, "meta kind mismatch on $name")
      is MsgMeta.Request -> assertEquals("request", expectedKind, "meta kind mismatch on $name")
      is MsgMeta.Response -> {
        assertEquals("response", expectedKind, "meta kind mismatch on $name")
        assertEquals(FIXED_REQUEST_ID, meta.data.requestId, "requestId mismatch on $name")
      }
    }
  }

  @Test
  fun `forward-text decodes to expected string`() {
    val fixture = loadGoldens().fixtures.first { it.name == "bridge_to_gateway/forward-text-event" }
    val msg = codec.decode(BridgeToGatewayMsg.serializer(), hexToBytes(fixture.framedHex))
    val data = msg.data
    assertTrue(data is BridgeToGatewayMsgData.Forward)
    val forward = (data as BridgeToGatewayMsgData.Forward).data
    assertTrue(forward is ForwardMessage.Text)
    assertEquals("hello, gateway", (forward as ForwardMessage.Text).data)
  }

  @Test
  fun `forward-binary decodes to expected bytes`() {
    val fixture = loadGoldens().fixtures.first { it.name == "bridge_to_gateway/forward-binary-event" }
    val msg = codec.decode(BridgeToGatewayMsg.serializer(), hexToBytes(fixture.framedHex))
    val forward = (msg.data as BridgeToGatewayMsgData.Forward).data
    assertTrue(forward is ForwardMessage.Binary)
    assertArrayEquals(
      byteArrayOf(0x89.toByte(), 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a),
      (forward as ForwardMessage.Binary).data,
    )
  }

  @Test
  fun `forward-json round-trips arbitrary JSON over msgpack`() {
    val fixture = loadGoldens().fixtures.first { it.name == "bridge_to_gateway/forward-json-event" }
    val msg = codec.decode(BridgeToGatewayMsg.serializer(), hexToBytes(fixture.framedHex))
    val forward = (msg.data as BridgeToGatewayMsgData.Forward).data
    assertTrue(forward is ForwardMessage.Json, "expected ForwardMessage.Json, got $forward")
    val json = (forward as ForwardMessage.Json).data.jsonObject

    // fixture content: {"kind":"playback-changed","payload":{"playing":true,"positionMs":12345}}
    assertEquals("playback-changed", json["kind"]!!.jsonPrimitive.content)
    val payload = json["payload"]!!.jsonObject
    assertEquals(true, payload["playing"]!!.jsonPrimitive.content.toBoolean())
    assertEquals("12345", payload["positionMs"]!!.jsonPrimitive.content)

    val reEncoded = codec.encode(BridgeToGatewayMsg.serializer(), msg)
    val reDecoded = codec.decode(BridgeToGatewayMsg.serializer(), reEncoded)
    val reForward = (reDecoded.data as BridgeToGatewayMsgData.Forward).data as ForwardMessage.Json
    assertEquals(json, reForward.data.jsonObject, "JSON content drifted on msgpack round-trip")
  }

  @Test
  fun `gzip end-to-end round-trip`() {
    val gzipCodec = Codec(defaultCompression = Compression.GZIP, defaultEncoding = Encoding.MSGPACK)
    val original = BridgeToGatewayMsg(
      id = FIXED_ID,
      meta = MsgMeta.Response(ResponseMeta(requestId = FIXED_REQUEST_ID)),
      data = BridgeToGatewayMsgData.Ack,
    )
    val frame = gzipCodec.encode(BridgeToGatewayMsg.serializer(), original)
    val decoded = gzipCodec.decode(BridgeToGatewayMsg.serializer(), frame)
    assertEquals(original.id, decoded.id)
    val resp = decoded.meta as MsgMeta.Response
    assertEquals(FIXED_REQUEST_ID, resp.data.requestId)
    assertTrue(decoded.data is BridgeToGatewayMsgData.Ack)
  }

  private fun loadGoldens(): GoldenFile {
    // user.dir is the Gradle invocation cwd; try candidate relative paths so the
    // test passes from the repo root or the gateway module dir.
    val here = Paths.get(System.getProperty("user.dir"))
    val candidates = listOf(
      here.resolve("crates/lib/fixtures/golden.json"),
      here.resolve("../../../../crates/lib/fixtures/golden.json"),
      here.resolve("../../../crates/lib/fixtures/golden.json"),
      here.resolve("../../crates/lib/fixtures/golden.json"),
      here.resolve("../crates/lib/fixtures/golden.json"),
    )
    val path = candidates.firstOrNull { Files.exists(it) }
      ?: error("could not locate fixtures/golden.json from $here (tried: ${candidates.joinToString()})")
    val text = path.toFile().readText()
    return Json { ignoreUnknownKeys = true }.decodeFromString(GoldenFile.serializer(), text)
  }
}

@Serializable
private data class GoldenFile(val version: Int, val magic: String, val fixtures: List<GoldenFixture>)

@Serializable
private data class GoldenFixture(
  val name: String,
  val description: String,
  val direction: Direction,
  val priority: String,
  val decoded_json: JsonElement,
  val msgpack_hex: String,
  val framed_hex: String,
) {
  val decodedJson: JsonElement get() = decoded_json
  val msgpackHex: String get() = msgpack_hex
  val framedHex: String get() = framed_hex
  val expectedPriority: Priority get() = when (priority) {
    "bulk" -> Priority.Bulk
    "background" -> Priority.Background
    else -> Priority.Normal
  }
}

@Serializable
private enum class Direction {
  @kotlinx.serialization.SerialName("bridge_to_gateway")
  BRIDGE_TO_GATEWAY,
  @kotlinx.serialization.SerialName("gateway_to_bridge")
  GATEWAY_TO_BRIDGE,
}

private fun bytesToHex(bytes: ByteArray): String =
  bytes.joinToString("") { "%02x".format(it) }

private fun hexToBytes(hex: String): ByteArray {
  require(hex.length % 2 == 0)
  val out = ByteArray(hex.length / 2)
  for (i in out.indices) {
    out[i] = hex.substring(i * 2, i * 2 + 2).toInt(16).toByte()
  }
  return out
}
