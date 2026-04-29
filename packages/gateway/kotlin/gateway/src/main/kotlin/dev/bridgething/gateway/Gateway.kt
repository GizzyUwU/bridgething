package dev.bridgething.gateway

import dev.bridgething.schema.BridgeToGatewayMsg
import dev.bridgething.schema.GatewayMsgMeta
import dev.bridgething.schema.GatewayToBridgeMsg
import dev.bridgething.schema.GatewayToBridgeMsgData
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import java.util.UUID
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

public sealed class GatewayEvent {
  public data class Connected(public val device: Device) : GatewayEvent()
  public data class Disconnected(public val deviceId: String) : GatewayEvent()
  public data class Message(
    public val deviceId: String,
    public val message: BridgeToGatewayMsg,
  ) : GatewayEvent()
  public data class DecodeError(
    public val deviceId: String,
    public val description: String,
  ) : GatewayEvent()
}

public sealed class GatewayException(message: String) : RuntimeException(message) {
  public class NotRunning : GatewayException("gateway not started")
  public class AlreadyRunning : GatewayException("gateway already started")
  public class RequestTimedOut : GatewayException("request timed out")
  public class Shutdown : GatewayException("gateway is shutting down")
}

/**
 * Typed phone-side facade over an [Adapter].
 *
 * Owns one [FrameAccumulator] per connected device, decodes incoming frames
 * into [BridgeToGatewayMsg], encodes outbound [GatewayToBridgeMsg] through
 * the shared [Codec], and tracks in-flight requests so callers can `await` a
 * matching response by id.
 */
public class BridgethingGateway(
  private val adapter: Adapter,
  private val codec: Codec = Codec(),
) {
  private val scope = CoroutineScope(
    SupervisorJob() + Dispatchers.Default + CoroutineName("bridgething-gateway")
  )
  private val mutex = Mutex()

  private val buffers = mutableMapOf<String, FrameAccumulator>()
  private val pendingRequests = mutableMapOf<UUID, CompletableDeferred<BridgeToGatewayMsg>>()
  private var consumerJob: Job? = null

  private val outboundEvents = Channel<GatewayEvent>(Channel.BUFFERED)
  public val events: Flow<GatewayEvent> = outboundEvents.receiveAsFlow()

  public suspend fun start() {
    mutex.withLock {
      if (consumerJob != null) throw GatewayException.AlreadyRunning()
      adapter.start()
      consumerJob = scope.launch {
        adapter.events.collect { event -> handleAdapterEvent(event) }
      }
    }
  }

  public suspend fun stop() {
    val job: Job?
    val deferreds: List<CompletableDeferred<BridgeToGatewayMsg>>
    mutex.withLock {
      job = consumerJob
      consumerJob = null
      deferreds = pendingRequests.values.toList()
      pendingRequests.clear()
      buffers.clear()
    }
    job?.cancelAndJoin()
    adapter.stop()
    deferreds.forEach { it.completeExceptionally(GatewayException.Shutdown()) }
    outboundEvents.close()
    scope.cancel()
  }

  public suspend fun disconnect(deviceId: String) {
    adapter.disconnect(deviceId)
  }

  /**
   * Encode and ship a fully-formed message. Caller is responsible for picking
   * `meta` ([GatewayMsgMeta.Command], [GatewayMsgMeta.Event], etc.). For
   * request/response, prefer [request] which handles id generation and
   * awaiting the matching response.
   */
  public suspend fun send(deviceId: String, message: GatewayToBridgeMsg) {
    val frame = codec.encode(GatewayToBridgeMsg.serializer(), message)
    adapter.send(deviceId, frame)
  }

  /**
   * Send a request and await the matching response by id. The wire id is
   * generated here and matched against
   * `BridgeToGatewayMsg.meta.response.requestId` on the way back; non-response
   * messages with the same id (shouldn't happen, but we don't trust the wire)
   * flow through the event stream as usual.
   */
  public suspend fun request(
    deviceId: String,
    data: GatewayToBridgeMsgData,
    timeout: Duration = 30.seconds,
  ): BridgeToGatewayMsg {
    val id = UUID.randomUUID()
    val msg = GatewayToBridgeMsg(
      id = id.toBytes(),
      meta = GatewayMsgMeta.Request,
      data = data,
    )
    val deferred = CompletableDeferred<BridgeToGatewayMsg>()
    mutex.withLock { pendingRequests[id] = deferred }

    return try {
      val frame = codec.encode(GatewayToBridgeMsg.serializer(), msg)
      adapter.send(deviceId, frame)
      withTimeout(timeout) { deferred.await() }
    } catch (_: TimeoutCancellationException) {
      mutex.withLock { pendingRequests.remove(id) }
      throw GatewayException.RequestTimedOut()
    } catch (e: Throwable) {
      mutex.withLock { pendingRequests.remove(id) }
      throw e
    }
  }

  private suspend fun handleAdapterEvent(event: AdapterEvent) {
    when (event) {
      is AdapterEvent.Connected -> {
        mutex.withLock { buffers[event.device.id] = FrameAccumulator() }
        outboundEvents.send(GatewayEvent.Connected(event.device))
      }
      is AdapterEvent.Disconnected -> {
        mutex.withLock { buffers.remove(event.deviceId) }
        outboundEvents.send(GatewayEvent.Disconnected(event.deviceId))
      }
      is AdapterEvent.Bytes -> ingest(event.deviceId, event.data)
    }
  }

  private suspend fun ingest(deviceId: String, chunk: ByteArray) {
    mutex.withLock {
      buffers.getOrPut(deviceId) { FrameAccumulator() }.append(chunk)
    }
    while (true) {
      val frame: ByteArray = try {
        mutex.withLock { buffers[deviceId]?.nextFrame() } ?: return
      } catch (e: Throwable) {
        mutex.withLock { buffers[deviceId] = FrameAccumulator() }
        outboundEvents.send(GatewayEvent.DecodeError(deviceId, e.message ?: e.toString()))
        return
      }
      val msg = try {
        codec.decode(BridgeToGatewayMsg.serializer(), frame)
      } catch (e: Throwable) {
        outboundEvents.send(GatewayEvent.DecodeError(deviceId, e.message ?: e.toString()))
        continue
      }
      val resp = msg.meta as? GatewayMsgMeta.Response
      val resolved = if (resp != null) {
        val requestId = uuidFromBytes(resp.data.requestId)
        val deferred = mutex.withLock { pendingRequests.remove(requestId) }
        deferred?.complete(msg) ?: false
      } else {
        false
      }
      if (!resolved) {
        outboundEvents.send(GatewayEvent.Message(deviceId, msg))
      }
    }
  }
}
