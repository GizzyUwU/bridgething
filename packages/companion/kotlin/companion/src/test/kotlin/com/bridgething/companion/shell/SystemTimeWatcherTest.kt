package com.bridgething.companion.shell

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import io.mockk.CapturingSlot
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

class SystemTimeWatcherTest {
    private fun broadcast(action: String): Intent = mockk<Intent> { every { getAction() } returns action }

    private fun capturingRegistration(context: Context): CapturingSlot<BroadcastReceiver> {
        val slot = slot<BroadcastReceiver>()
        every { context.registerReceiver(capture(slot), any<IntentFilter>()) } returns null
        every { context.registerReceiver(capture(slot), any<IntentFilter>(), any<Int>()) } returns null
        return slot
    }

    @Test
    fun clockAndTimeZoneBroadcastsReachTheSessionAndStopUnregisters() {
        val context = mockk<Context>(relaxed = true)
        every { context.applicationContext } returns context
        val receiver = capturingRegistration(context)

        val fired = LinkedBlockingQueue<String>()
        val watcher = SystemTimeWatcher(context) { fired.add("timeChanged") }
        watcher.start()

        receiver.captured.onReceive(context, broadcast(Intent.ACTION_TIMEZONE_CHANGED))
        assertEquals("timeChanged", fired.poll(1, TimeUnit.SECONDS))

        receiver.captured.onReceive(context, broadcast(Intent.ACTION_TIME_CHANGED))
        assertEquals("timeChanged", fired.poll(1, TimeUnit.SECONDS))

        receiver.captured.onReceive(context, broadcast(Intent.ACTION_SCREEN_ON))
        assertNull(fired.poll(200, TimeUnit.MILLISECONDS))

        watcher.stop()
        verify { context.unregisterReceiver(receiver.captured) }
    }
}
