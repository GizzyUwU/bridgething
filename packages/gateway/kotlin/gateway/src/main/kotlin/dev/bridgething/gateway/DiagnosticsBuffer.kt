package dev.bridgething.gateway

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/**
 * One record in the diagnostics ring buffer.
 */
public data class DiagRecord(
  val seq: Long,
  val timestampMs: Double,
  val kind: Kind,
  val deviceId: String? = null,
  val direction: Direction? = null,
  val frameKind: FrameKind? = null,
  val surface: String? = null,
  val byteSize: Int? = null,
  val requestId: String? = null,
  val latencyMs: Double? = null,
  val level: String? = null,
  val target: String? = null,
  val message: String? = null,
  val category: String? = null,
  val detail: String? = null,
  val fields: List<Pair<String, String>>? = null,
  val payload: String? = null,
) {
  public enum class Kind { FRAME, LOG, BREADCRUMB }
  public enum class Direction { OUTBOUND, INBOUND }
  public enum class FrameKind { REQUEST, RESPONSE, EVENT, COMMAND }

  internal val approxBytes: Int
    get() {
      var n = 96
      n += (deviceId?.length ?: 0) + (surface?.length ?: 0) + (requestId?.length ?: 0)
      n += (level?.length ?: 0) + (target?.length ?: 0) + (message?.length ?: 0)
      n += (category?.length ?: 0) + (detail?.length ?: 0) + (payload?.length ?: 0)
      fields?.forEach { n += it.first.length + it.second.length + 16 }
      return n
    }
}

/**
 * Process-wide, always-on diagnostics ring buffer: wire frames (from the gateway
 * send/ingest tap), companion native log lines, and structured breadcrumbs (from
 * the glue augmentation path). FIFO eviction keeps it under a byte budget. A
 * singleton because the producers are bootstrapped at process scope and the host
 * reads/subscribes once; mirrors the existing process-wide bridge registries.
 */
public object DiagnosticsBuffer {
  private const val BYTE_BUDGET = 8 * 1024 * 1024

  private val lock = Any()
  private val records = ArrayDeque<DiagRecord>()
  private var bytes = 0
  private var seqCounter = 0L

  private val _stream = MutableSharedFlow<DiagRecord>(
    extraBufferCapacity = 1024,
    onBufferOverflow = BufferOverflow.DROP_OLDEST,
  )

  /** Live stream of records inserted after subscription. The foreground pull uses
   *  [tail]; this carries everything after. The consumer reconciles via `seq`. */
  public val stream: SharedFlow<DiagRecord> = _stream.asSharedFlow()

  /** Most-recent [limit] records, oldest-first. */
  public fun tail(limit: Int): List<DiagRecord> = synchronized(lock) {
    if (limit >= records.size) records.toList() else records.toList().takeLast(limit)
  }

  public fun recordFrame(
    deviceId: String,
    direction: DiagRecord.Direction,
    frameKind: DiagRecord.FrameKind,
    surface: String,
    byteSize: Int,
    requestId: String?,
    latencyMs: Double?,
    payload: String?,
  ) {
    insert { seq, ts ->
      DiagRecord(
        seq = seq, timestampMs = ts, kind = DiagRecord.Kind.FRAME,
        deviceId = deviceId, direction = direction, frameKind = frameKind,
        surface = surface, byteSize = byteSize, requestId = requestId, latencyMs = latencyMs,
        payload = payload,
      )
    }
  }

  public fun recordLog(level: String, target: String?, message: String) {
    insert { seq, ts ->
      DiagRecord(
        seq = seq, timestampMs = ts, kind = DiagRecord.Kind.LOG,
        level = level, target = target, message = message,
      )
    }
  }

  public fun recordBreadcrumb(
    category: String,
    detail: String,
    fields: List<Pair<String, String>>? = null,
  ) {
    insert { seq, ts ->
      DiagRecord(
        seq = seq, timestampMs = ts, kind = DiagRecord.Kind.BREADCRUMB,
        category = category, detail = detail, fields = fields,
      )
    }
  }

  private inline fun insert(build: (Long, Double) -> DiagRecord) {
    val record = synchronized(lock) {
      seqCounter += 1
      val r = build(seqCounter, System.currentTimeMillis().toDouble())
      records.addLast(r)
      bytes += r.approxBytes
      while (bytes > BYTE_BUDGET && records.size > 1) {
        bytes -= records.removeFirst().approxBytes
      }
      r
    }
    _stream.tryEmit(record)
  }
}
