package com.bridgething.companion.shell

import android.content.Context
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.WakeReason

class IntentDeviceWakerTest {
    @Test
    fun everyReasonBroadcastsTheMediaKeyPair() {
        val context = mockk<Context>(relaxed = true)
        every { context.applicationContext } returns context

        val waker = IntentDeviceWaker(context)
        waker.wakeDevice(WakeReason.USER_PLAY)
        waker.wakeDevice(WakeReason.CONNECT_RESUME)

        verify(exactly = 4) { context.sendBroadcast(any()) }
    }
}
