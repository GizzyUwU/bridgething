package dev.bridgething.companion

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow

/**
 * Process-wide bridge between the OS-constructed [BridgethingInCallService] and the companion's
 * [AndroidPhoneBackend]. The telecom framework constructs the InCallService independently of the
 * app's coroutine scope; buffered drop-oldest so tryEmit never blocks a telecom callback.
 */
public object PhoneBridgeRegistry {
    @Volatile
    public var service: BridgethingInCallService? = null

    public val events: MutableSharedFlow<PhoneOutEvent> = MutableSharedFlow(
        replay = 0,
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
}
