package dev.bridgething.companion

import dev.bridgething.gateway.AdapterEvent
import dev.bridgething.gateway.Codec
import dev.bridgething.gateway.Compression
import dev.bridgething.gateway.Device
import dev.bridgething.gateway.Encoding
import dev.bridgething.schema.BridgeToGatewayMsg
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.GatewayToBridgeMsg
import dev.bridgething.schema.MsgMeta
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeout
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * Drives the companion exactly as the bridgething daemon would: encodes a
 * `BridgeToGatewayMsg` and pushes it through the [FakeAdapter], then reads the
 * companion's `GatewayToBridgeMsg` frames back out of `sentFrames`, correlating
 * responses by `requestId`. Non-response frames land in a buffer that
 * [waitOutbound] drains.
 */
class WireDriver(
    private val adapter: FakeAdapter,
    val deviceId: String = "carthing-test",
    private val codec: Codec = Codec(defaultCompression = Compression.NONE, defaultEncoding = Encoding.MSGPACK),
) {
    private val pending = ConcurrentHashMap<UUID, CompletableDeferred<GatewayToBridgeMsg>>()
    private val outbound = Channel<GatewayToBridgeMsg>(Channel.UNLIMITED)
    // non-matching frames are retained so a later waitOutbound finds out-of-order events
    private val buffered = ArrayDeque<GatewayToBridgeMsg>()
    private var pump: Job? = null

    fun start(scope: CoroutineScope) {
        pump = scope.launch {
            for ((_, frame) in adapter.sentFrames) {
                val msg = runCatching { codec.decode(GatewayToBridgeMsg.serializer(), frame) }.getOrNull() ?: continue
                val meta = msg.meta
                if (meta is MsgMeta.Response) {
                    pending.remove(meta.data.requestId)?.complete(msg)
                } else {
                    outbound.trySend(msg)
                }
            }
        }
    }

    suspend fun connect(name: String = "Car Thing") {
        adapter.simulate(AdapterEvent.Connected(Device(deviceId, name)))
    }

    suspend fun request(data: BridgeToGatewayMsgData, timeout: Duration = 5.seconds): GatewayToBridgeMsg {
        val id = UUID.randomUUID()
        val deferred = CompletableDeferred<GatewayToBridgeMsg>()
        pending[id] = deferred
        val frame = codec.encode(
            BridgeToGatewayMsg.serializer(),
            BridgeToGatewayMsg(id = id, meta = MsgMeta.Request, data = data),
        )
        adapter.simulate(AdapterEvent.Bytes(deviceId, frame))
        return withTimeout(timeout) { deferred.await() }
    }

    suspend fun send(data: BridgeToGatewayMsgData, meta: MsgMeta = MsgMeta.Command) {
        val frame = codec.encode(
            BridgeToGatewayMsg.serializer(),
            BridgeToGatewayMsg(id = UUID.randomUUID(), meta = meta, data = data),
        )
        adapter.simulate(AdapterEvent.Bytes(deviceId, frame))
    }

    suspend fun waitOutbound(
        timeout: Duration = 5.seconds,
        predicate: (GatewayToBridgeMsg) -> Boolean,
    ): GatewayToBridgeMsg = withTimeout(timeout) {
        val hit = buffered.indexOfFirst(predicate)
        if (hit >= 0) return@withTimeout buffered.removeAt(hit)
        var msg = outbound.receive()
        while (!predicate(msg)) {
            buffered.addLast(msg)
            msg = outbound.receive()
        }
        msg
    }

    fun stop() {
        pump?.cancel()
    }
}
