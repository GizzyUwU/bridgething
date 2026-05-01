package dev.bridgething.gateway

import dev.bridgething.schema.AssetClear
import dev.bridgething.schema.AssetPush
import dev.bridgething.schema.AssetRetention
import dev.bridgething.schema.AuthorityClaim
import dev.bridgething.schema.AuthorityRelease
import dev.bridgething.schema.BridgeToGatewayMsg
import dev.bridgething.schema.CompanionAuthorityScope
import dev.bridgething.schema.GatewayMsgMeta
import dev.bridgething.schema.GatewayToBridgeAssetMsg
import dev.bridgething.schema.GatewayToBridgeAuthorityMsg
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
    adapter.send(deviceId, frame)
  }

  /** Bulk-priority shorthand for [send]. */
  public suspend fun sendBulk(deviceId: String, message: GatewayToBridgeMsg) {
    send(deviceId, message, priority = Priority.Bulk)
  }

  /**
   * Push a binary blob into the daemon's asset cache. Companion-managed
   * retention - the daemon enforces it but expects companion to clear
   * pinned assets explicitly when no longer needed.
   *
   * Bulk priority is the right default: asset blobs may be sizable
   * (full-size album art, glyph atlases) and shouldn't preempt latency-
   * sensitive traffic like NowPlaying deltas.
   */
  public suspend fun pushAsset(
    deviceId: String,
    id: String,
    bytes: ByteArray,
    mime: String? = null,
    retention: AssetRetention = AssetRetention.Lru,
  ) {
    val push = AssetPush(id = id, bytes = bytes, mime = mime, retention = retention)
    val msg = GatewayToBridgeMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Event,
      data = GatewayToBridgeMsgData.Asset(GatewayToBridgeAssetMsg.Push(push)),
    )
    send(deviceId, msg, priority = Priority.Bulk)
  }

  /** Drop a previously pushed asset. */
  public suspend fun clearAsset(deviceId: String, id: String) {
    val msg = GatewayToBridgeMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Event,
      data = GatewayToBridgeMsgData.Asset(GatewayToBridgeAssetMsg.Clear(AssetClear(id = id))),
    )
    send(deviceId, msg)
  }

  /** Claim authority over a scope. Idempotent; re-issue to refresh staleness. */
  public suspend fun claimAuthority(deviceId: String, scope: CompanionAuthorityScope) {
    val msg = GatewayToBridgeMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Event,
      data = GatewayToBridgeMsgData.Authority(GatewayToBridgeAuthorityMsg.Claim(AuthorityClaim(scope = scope))),
    )
    send(deviceId, msg)
  }

  /** Release authority over a scope. */
  public suspend fun releaseAuthority(deviceId: String, scope: CompanionAuthorityScope) {
    val msg = GatewayToBridgeMsg(
      id = UUID.randomUUID().toBytes(),
      meta = GatewayMsgMeta.Event,
      data = GatewayToBridgeMsgData.Authority(GatewayToBridgeAuthorityMsg.Release(AuthorityRelease(scope = scope))),
    )
    send(deviceId, msg)
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
