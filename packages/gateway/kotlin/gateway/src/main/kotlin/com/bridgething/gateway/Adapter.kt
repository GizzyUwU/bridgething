package com.bridgething.gateway

import kotlinx.coroutines.flow.Flow

public data class Device(public val id: String, public val name: String)

public sealed class AdapterEvent {
    public data class Connected(public val device: Device) : AdapterEvent()
    public data class Disconnected(public val deviceId: String) : AdapterEvent()

    public data class LinkFailed(public val device: Device, public val reason: String) : AdapterEvent()
    public class Bytes(
        public val deviceId: String,
        public val data: ByteArray,
    ) : AdapterEvent() {
        override fun equals(other: Any?): Boolean {
            if (this === other) return true
            if (other !is Bytes) return false
            return deviceId == other.deviceId && data.contentEquals(other.data)
        }

        override fun hashCode(): Int = 31 * deviceId.hashCode() + data.contentHashCode()
        override fun toString(): String = "Bytes(deviceId=$deviceId, ${data.size} bytes)"
    }
}

public sealed class AdapterException(message: String) : RuntimeException(message) {
    public class NotStarted : AdapterException("adapter not started")
    public class UnknownDevice(deviceId: String) : AdapterException("unknown device: $deviceId")
    public class SendFailed(detail: String) : AdapterException("send failed: $detail")
    public class TransportFailure(detail: String) : AdapterException("transport failure: $detail")

    public class NotBonded(deviceId: String) : AdapterException("device not bonded: $deviceId")
}

public interface Adapter {
    public val events: Flow<AdapterEvent>

    public suspend fun start()
    public suspend fun stop()
    public suspend fun disconnect(deviceId: String)
    public suspend fun send(deviceId: String, frame: ByteArray)
    public suspend fun reconnect(deviceId: String) {}
}
