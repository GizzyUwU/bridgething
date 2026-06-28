package com.bridgething.companion

public data class DeviceLogRecord(
    val seq: Long,
    val timestampMs: Double,
    val level: String,
    val message: String,
)

/**
 * Process-wide ring of device-log lines (daemon tracing forwarded over the gateway,
 * plus companion-origin lines). The live `logObserver` path carries entries in
 * real-time while foregrounded; this durable tail lets the RN side backfill the gap
 * opened while the app is backgrounded and the JS thread is suspended.
 */
public object DeviceLogRing {
    private const val LIMIT = 2000

    private val lock = Any()
    private val records = ArrayDeque<DeviceLogRecord>()
    private var seqCounter = 0L

    public fun push(level: String, message: String) {
        synchronized(lock) {
            seqCounter += 1
            records.addLast(DeviceLogRecord(seqCounter, System.currentTimeMillis().toDouble(), level, message))
            while (records.size > LIMIT) records.removeFirst()
        }
    }

    public fun tail(limit: Int): List<DeviceLogRecord> = synchronized(lock) {
        if (limit >= records.size) records.toList() else records.toList().takeLast(limit)
    }
}
