package dev.bridgething.gateway

import dev.bridgething.schema.BridgeToGatewayMsg
import dev.bridgething.schema.MsgMeta
import dev.bridgething.schema.GatewayToBridgeMsg
import dev.bridgething.schema.GatewayToBridgeMsgData
import dev.bridgething.schema.Priority
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
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
  private val requestSentAt = mutableMapOf<UUID, Double>()
  private var consumerJob: Job? = null

  // Broadcast, not fan-out: every dispatcher collector must see every event. A
  // `Channel.receiveAsFlow()` distributes each event to only ONE of the competing
  // collectors, so the companion's many surface dispatchers would drop most events.
  // replay lets a dispatcher that subscribes slightly after start() still catch recent
  // events; all companion dispatchers subscribe at start before any peer connects, so
  // replay never re-delivers in practice.
  private val outboundEvents = MutableSharedFlow<GatewayEvent>(
    replay = 16,
    extraBufferCapacity = 256,
    onBufferOverflow = BufferOverflow.SUSPEND,
  )
  public val events: Flow<GatewayEvent> = outboundEvents.asSharedFlow()

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
    scope.cancel()
  }

  public suspend fun disconnect(deviceId: String) {
    adapter.disconnect(deviceId)
  }

  public suspend fun reconnect(deviceId: String) {
    adapter.reconnect(deviceId)
  }

  /** Snapshot of currently connected peer ids. */
  public suspend fun connectedDeviceIds(): List<String> = mutex.withLock { buffers.keys.toList() }

  /**
   * Encode and ship a fully-formed message. Caller is responsible for picking
   * `meta` ([MsgMeta.Command], [MsgMeta.Event], etc.). For
   * request/response, prefer [request] which handles id generation and
   * awaiting the matching response.
   *
   * `priority` is a transport-level scheduling hint - Bulk yields to Normal at
   * frame boundaries so latency-sensitive traffic interleaves between long
   * bulk transfers (file/OTA chunks). Default is [Priority.Normal].
   */
  public suspend fun send(
    deviceId: String,
    message: GatewayToBridgeMsg,
    priority: Priority = Priority.Normal,
  ) {
    val frame = codec.encode(GatewayToBridgeMsg.serializer(), message, priority = priority)
    DiagnosticsBuffer.recordFrame(
      deviceId = deviceId,
      direction = DiagRecord.Direction.OUTBOUND,
      frameKind = diagFrameKind(message.meta),
      surface = diagSurface(message.data),
      byteSize = frame.size,
      requestId = diagRequestId(message.meta, message.id),
      latencyMs = null,
    )
    adapter.send(deviceId, frame)
  }

  /** Bulk-priority shorthand for [send]. */
  public suspend fun sendBulk(deviceId: String, message: GatewayToBridgeMsg) {
    send(deviceId, message, priority = Priority.Bulk)
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
      id = id,
      meta = MsgMeta.Request,
      data = data,
    )
    val deferred = CompletableDeferred<BridgeToGatewayMsg>()
    mutex.withLock {
      pendingRequests[id] = deferred
      requestSentAt[id] = System.currentTimeMillis().toDouble()
    }

    return try {
      val frame = codec.encode(GatewayToBridgeMsg.serializer(), msg)
      DiagnosticsBuffer.recordFrame(
        deviceId = deviceId,
        direction = DiagRecord.Direction.OUTBOUND,
        frameKind = DiagRecord.FrameKind.REQUEST,
        surface = diagSurface(data),
        byteSize = frame.size,
        requestId = id.toString(),
        latencyMs = null,
      )
      adapter.send(deviceId, frame)
      withTimeout(timeout) { deferred.await() }
    } catch (_: TimeoutCancellationException) {
      mutex.withLock { pendingRequests.remove(id); requestSentAt.remove(id) }
      throw GatewayException.RequestTimedOut()
    } catch (e: Throwable) {
      mutex.withLock { pendingRequests.remove(id); requestSentAt.remove(id) }
      throw e
    }
  }

  private suspend fun handleAdapterEvent(event: AdapterEvent) {
    when (event) {
      is AdapterEvent.Connected -> {
        mutex.withLock { buffers[event.device.id] = FrameAccumulator() }
        outboundEvents.emit(GatewayEvent.Connected(event.device))
      }
      is AdapterEvent.Disconnected -> {
        mutex.withLock { buffers.remove(event.deviceId) }
        outboundEvents.emit(GatewayEvent.Disconnected(event.deviceId))
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
        outboundEvents.emit(GatewayEvent.DecodeError(deviceId, e.message ?: e.toString()))
        return
      }
      val msg = try {
        codec.decode(BridgeToGatewayMsg.serializer(), frame)
      } catch (e: Throwable) {
        outboundEvents.emit(GatewayEvent.DecodeError(deviceId, e.message ?: e.toString()))
        continue
      }
      val resp = msg.meta as? MsgMeta.Response
      var latencyMs: Double? = null
      val reqId: String? = when {
        resp != null -> resp.data.requestId.toString()
        msg.meta is MsgMeta.Request -> msg.id.toString()
        else -> null
      }
      if (resp != null) {
        val sentAt = mutex.withLock { requestSentAt.remove(resp.data.requestId) }
        if (sentAt != null) latencyMs = System.currentTimeMillis().toDouble() - sentAt
      }
      DiagnosticsBuffer.recordFrame(
        deviceId = deviceId,
        direction = DiagRecord.Direction.INBOUND,
        frameKind = diagFrameKind(msg.meta),
        surface = diagSurface(msg.data),
        byteSize = frame.size,
        requestId = reqId,
        latencyMs = latencyMs,
      )
      val resolved = if (resp != null) {
        val deferred = mutex.withLock { pendingRequests.remove(resp.data.requestId) }
        deferred?.complete(msg) ?: false
      } else {
        false
      }
      if (!resolved) {
        outboundEvents.emit(GatewayEvent.Message(deviceId, msg))
      }
    }
  }
}

private fun diagFrameKind(meta: MsgMeta): DiagRecord.FrameKind = when (meta) {
  is MsgMeta.Command -> DiagRecord.FrameKind.COMMAND
  is MsgMeta.Event -> DiagRecord.FrameKind.EVENT
  is MsgMeta.Request -> DiagRecord.FrameKind.REQUEST
  is MsgMeta.Response -> DiagRecord.FrameKind.RESPONSE
}

private fun diagRequestId(meta: MsgMeta, id: UUID): String? = when (meta) {
  is MsgMeta.Request -> id.toString()
  is MsgMeta.Response -> meta.data.requestId.toString()
  else -> null
}

private fun diagSurface(data: Any): String = data::class.simpleName ?: "unknown"
