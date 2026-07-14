package com.bridgething.gateway

import kotlinx.coroutines.flow.Flow

/**
 * Identifier and human-readable name for a connected bridgething peer.
 *
 * `id` is opaque to the gateway and only meaningful to the underlying adapter
 * (BluetoothDevice address on Android, EAAccessory.serialNumber on iOS, BLE
 * peripheral UUID on the cross-platform path). Pass it back to [Adapter.send]
 * / [Adapter.disconnect] to address that specific peer.
 */
public data class Device(public val id: String, public val name: String)

/**
 * Raw byte-level events surfaced by an [Adapter] to the gateway. The gateway
 * accumulates [Bytes] chunks per device into framed payloads - adapters do
 * not need to align chunks to frame boundaries.
 */
public sealed class AdapterEvent {
    public data class Connected(public val device: Device) : AdapterEvent()
    public data class Disconnected(public val deviceId: String) : AdapterEvent()

    /**
     * The transport is connected to the peer at the link level (e.g. an ACL
     * link exists on Android) but a bridgething session could not be brought
     * up over it - the daemon isn't reachable on its service. Distinct from
     * [Disconnected], which means the peer is simply gone / out of range.
     */
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

    /**
     * The peer is not bonded, so the transport refused to open a link to it.
     *
     * On Android, connecting an RFCOMM socket to an unbonded device makes the
     * OS start pairing on our behalf. That is never what a background reconnect
     * wants, so it is a hard error rather than a retryable one: callers must
     * bond first (explicitly, in the foreground) and connect afterwards. Retry
     * loops must treat this as terminal - retrying is what produces duplicate
     * system pairing dialogs.
     */
    public class NotBonded(deviceId: String) : AdapterException("device not bonded: $deviceId")
}

/**
 * Byte-level transport contract. Implementations plumb a specific Bluetooth
 * stack (BluetoothSocket on Android, EAAccessory on iOS, BLE elsewhere) and
 * emit raw chunks; framing, gzip, and msgpack live one layer up in the
 * gateway, not here.
 *
 * Multi-device by design: a single [Adapter] instance can manage several
 * concurrent peers, addressed by the opaque [Device.id].
 */
public interface Adapter {
    public val events: Flow<AdapterEvent>

    public suspend fun start()
    public suspend fun stop()
    public suspend fun disconnect(deviceId: String)
    public suspend fun send(deviceId: String, frame: ByteArray)
    public suspend fun reconnect(deviceId: String) {}
}
