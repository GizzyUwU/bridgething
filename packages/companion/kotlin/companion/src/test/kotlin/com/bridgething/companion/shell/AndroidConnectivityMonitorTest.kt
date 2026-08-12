package com.bridgething.companion.shell

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.ConnectivityInbox
import uniffi.bridgething_companion.NoHandle

private class RecordingConnectivityInbox : ConnectivityInbox(NoHandle) {
    val edges = LinkedBlockingQueue<Boolean>()

    override fun onChanged(online: Boolean) {
        edges.add(online)
    }
}

class AndroidConnectivityMonitorTest {
    @Test
    fun availabilityTransitionsReachTheInboxAndStopUnregisters() {
        val manager = mockk<ConnectivityManager>(relaxed = true)
        val context = mockk<Context>(relaxed = true)
        every { context.applicationContext } returns context
        every { context.getSystemService(Context.CONNECTIVITY_SERVICE) } returns manager

        val registered = slot<ConnectivityManager.NetworkCallback>()
        every { manager.registerDefaultNetworkCallback(capture(registered)) } returns Unit

        val monitor = AndroidConnectivityMonitor(context)
        val inbox = RecordingConnectivityInbox()
        monitor.start(inbox)

        val network = mockk<Network>()
        registered.captured.onAvailable(network)
        registered.captured.onLost(network)
        registered.captured.onAvailable(network)
        assertEquals(true, inbox.edges.poll(1, TimeUnit.SECONDS))
        assertEquals(false, inbox.edges.poll(1, TimeUnit.SECONDS))
        assertEquals(true, inbox.edges.poll(1, TimeUnit.SECONDS))

        monitor.stop()
        verify { manager.unregisterNetworkCallback(registered.captured) }
    }
}
