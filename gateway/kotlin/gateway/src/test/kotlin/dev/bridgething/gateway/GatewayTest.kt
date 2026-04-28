package dev.bridgething.gateway

import dev.bridgething.schema.BridgeToGatewayMsg
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.ForwardMessage
import dev.bridgething.schema.GatewayMsgMeta
import dev.bridgething.schema.GatewayToBridgeFileMsg
import dev.bridgething.schema.GatewayToBridgeMsg
import dev.bridgething.schema.GatewayToBridgeMsgData
import dev.bridgething.schema.ResponseMeta
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.UUID
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class GatewayTest {
  private val codec = Codec(defaultCompression = Compression.NONE, defaultEncoding = Encoding.MSGPACK)
  private val testDevice = Device(id = "carthing-1", name = "Car Thing")

  @Test
  fun `forwards connect and disconnect events`() = runBlocking {
    val adapter = MockAdapter()
    val gateway = BridgethingGateway(adapter, codec)
    gateway.start()

    adapter.simulate(AdapterEvent.Connected(testDevice))
    adapter.simulate(AdapterEvent.Disconnected(testDevice.id))

    val received = withTimeout(2.seconds) { gateway.events.take(2).toList() }

    assertEquals(GatewayEvent.Connected(testDevice), received[0])
    assertEquals(GatewayEvent.Disconnected(testDevice.id), received[1])

    gateway.stop()
  }

  @Test
  fun `decodes incoming frames into messages`() = runBlocking {
    val adapter = MockAdapter()
    val gateway = BridgethingGateway(adapter, codec)
    gateway.start()

    val original = BridgeToGatewayMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Event,
      data = BridgeToGatewayMsgData.Forward(ForwardMessage.Text("hello, gateway")),
    )
    val frame = codec.encode(BridgeToGatewayMsg.serializer(), original)

    adapter.simulate(AdapterEvent.Connected(testDevice))
    adapter.simulate(AdapterEvent.Bytes(testDevice.id, frame))

    val received = withTimeout(2.seconds) { gateway.events.take(2).toList() }

    assertTrue(received[0] is GatewayEvent.Connected)
    val msgEvent = received[1] as GatewayEvent.Message
    assertEquals(testDevice.id, msgEvent.deviceId)
    assertArrayEquals(original.id, msgEvent.message.id)
    val forward = (msgEvent.message.data as BridgeToGatewayMsgData.Forward).data as ForwardMessage.Text
    assertEquals("hello, gateway", forward.data)

    gateway.stop()
  }

  @Test
  fun `reassembles frames across chunks`() = runBlocking {
    val adapter = MockAdapter()
    val gateway = BridgethingGateway(adapter, codec)
    gateway.start()

    val original = BridgeToGatewayMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Command,
      data = BridgeToGatewayMsgData.Ack,
    )
    val frame = codec.encode(BridgeToGatewayMsg.serializer(), original)
    val mid = frame.size / 2

    adapter.simulate(AdapterEvent.Connected(testDevice))
    adapter.simulate(AdapterEvent.Bytes(testDevice.id, frame.copyOfRange(0, mid)))
    adapter.simulate(AdapterEvent.Bytes(testDevice.id, frame.copyOfRange(mid, frame.size)))

    val received = withTimeout(2.seconds) { gateway.events.take(2).toList() }
    val msgEvent = received[1] as GatewayEvent.Message
    assertArrayEquals(original.id, msgEvent.message.id)

    gateway.stop()
  }

  @Test
  fun `send encodes and forwards to adapter`() = runBlocking {
    val adapter = MockAdapter()
    val gateway = BridgethingGateway(adapter, codec)
    gateway.start()

    val outbound = GatewayToBridgeMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Command,
      data = GatewayToBridgeMsgData.File(GatewayToBridgeFileMsg.List),
    )
    gateway.send(testDevice.id, outbound)

    val (deviceId, sentFrame) = withTimeout(2.seconds) { adapter.sentFrames.receive() }
    assertEquals(testDevice.id, deviceId)
    val decoded = codec.decode(GatewayToBridgeMsg.serializer(), sentFrame)
    assertArrayEquals(outbound.id, decoded.id)

    gateway.stop()
  }

  @Test
  fun `request response correlation`() = runBlocking {
    val adapter = MockAdapter()
    val gateway = BridgethingGateway(adapter, codec)
    gateway.start()
    adapter.simulate(AdapterEvent.Connected(testDevice))

    val pending = async {
      gateway.request(
        testDevice.id,
        GatewayToBridgeMsgData.File(GatewayToBridgeFileMsg.List),
      )
    }

    val (sentDevice, sentFrame) = withTimeout(2.seconds) { adapter.sentFrames.receive() }
    assertEquals(testDevice.id, sentDevice)
    val request = codec.decode(GatewayToBridgeMsg.serializer(), sentFrame)
    assertTrue(request.meta is GatewayMsgMeta.Request)

    val response = BridgeToGatewayMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Response(ResponseMeta(request.id)),
      data = BridgeToGatewayMsgData.Ack,
    )
    adapter.simulate(
      AdapterEvent.Bytes(testDevice.id, codec.encode(BridgeToGatewayMsg.serializer(), response))
    )

    val result = pending.await()
    val resp = result.meta as GatewayMsgMeta.Response
    assertArrayEquals(request.id, resp.data.requestId)

    gateway.stop()
  }

  @Test
  fun `request times out`() = runBlocking {
    val adapter = MockAdapter()
    val gateway = BridgethingGateway(adapter, codec)
    gateway.start()
    adapter.simulate(AdapterEvent.Connected(testDevice))

    val outcome = runCatching {
      gateway.request(
        testDevice.id,
        GatewayToBridgeMsgData.File(GatewayToBridgeFileMsg.List),
        timeout = 100.milliseconds,
      )
    }
    assertTrue(outcome.exceptionOrNull() is GatewayException.RequestTimedOut)

    gateway.stop()
  }
}
