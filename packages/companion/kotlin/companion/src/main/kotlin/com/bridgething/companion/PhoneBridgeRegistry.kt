package com.bridgething.companion

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import uniffi.bridgething_companion.CommunicationsState
import uniffi.bridgething_companion.PhoneCall
import uniffi.bridgething_companion.PhoneCallEnded
import uniffi.bridgething_companion.PhoneState

public sealed interface PhoneOutEvent {
    public data class CallStarted(val call: PhoneCall) : PhoneOutEvent
    public data class CallUpdated(val call: PhoneCall) : PhoneOutEvent
    public data class CallEnded(val ended: PhoneCallEnded) : PhoneOutEvent
    public data class Snapshot(val state: PhoneState) : PhoneOutEvent
    public data class Communications(val state: CommunicationsState) : PhoneOutEvent
}

public object PhoneBridgeRegistry {
    @Volatile
    public var service: BridgethingInCallService? = null

    public val events: MutableSharedFlow<PhoneOutEvent> = MutableSharedFlow(
        replay = 0,
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
}
