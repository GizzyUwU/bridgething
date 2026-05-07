package dev.bridgething.gateway

import dev.bridgething.schema.BridgeToGatewayMsg
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.ForwardMessage
import dev.bridgething.schema.MsgMeta
import dev.bridgething.schema.GatewayToBridgeMsg
import dev.bridgething.schema.GatewayToBridgeMsgData
import dev.bridgething.schema.Priority
import dev.bridgething.schema.ResponseMeta
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
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
        val reDecoded = codec.decode(GatewayToBridgeMsg.serializer(), reEncoded)
        assertEquals(msg.id, reDecoded.id, "round-trip id changed on ${fixture.name}")
      }
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

    // The decoded structure should match what Rust serde_json::to_value emitted
    // in the fixture: {"kind":"playback-changed","payload":{"playing":true,"positionMs":12345}}
    assertEquals("playback-changed", json["kind"]!!.jsonPrimitive.content)
    val payload = json["payload"]!!.jsonObject
    assertEquals(true, payload["playing"]!!.jsonPrimitive.content.toBoolean())
    assertEquals("12345", payload["positionMs"]!!.jsonPrimitive.content)

    // Round-trip back through the codec to confirm the JsonElement re-encodes
    // and re-decodes losslessly.
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

  // MARK: - fixture loading

  private fun loadGoldens(): GoldenFile {
    // user.dir is the Gradle invocation cwd. Try a handful of relative paths
    // so the test passes whether the test is invoked from the repo root, the
    // gateway/kotlin/gateway module dir, or anywhere in between.
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
  val expectedPriority: Priority get() = if (priority == "bulk") Priority.Bulk else Priority.Normal
}

@Serializable
private enum class Direction {
  @kotlinx.serialization.SerialName("bridge_to_gateway")
  BRIDGE_TO_GATEWAY,
  @kotlinx.serialization.SerialName("gateway_to_bridge")
  GATEWAY_TO_BRIDGE,
}

private fun hexToBytes(hex: String): ByteArray {
  require(hex.length % 2 == 0)
  val out = ByteArray(hex.length / 2)
  for (i in out.indices) {
    out[i] = hex.substring(i * 2, i * 2 + 2).toInt(16).toByte()
  }
  return out
}
