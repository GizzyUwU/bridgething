package com.bridgething.companion

import com.bridgething.gateway.AdapterEvent
import com.bridgething.gateway.Codec
import com.bridgething.gateway.Compression
import com.bridgething.gateway.Device
import com.bridgething.gateway.Encoding
import com.bridgething.schema.BridgeToGatewayMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.GatewayToBridgeMsg
import com.bridgething.schema.MsgMeta
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

class WireDriver(
    private val adapter: FakeAdapter,
    val deviceId: String = "carthing-test",
    private val codec: Codec = Codec(defaultCompression = Compression.NONE, defaultEncoding = Encoding.MSGPACK),
) {
    private val pending = ConcurrentHashMap<UUID, CompletableDeferred<GatewayToBridgeMsg>>()
    private val outbound = Channel<GatewayToBridgeMsg>(Channel.UNLIMITED)
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
