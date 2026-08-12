package com.bridgething.companion.shell

import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import io.mockk.every
import io.mockk.mockk
import java.io.ByteArrayOutputStream
import java.io.PipedInputStream
import java.io.PipedOutputStream
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.LinkDevice
import uniffi.bridgething_companion.LinkInbox
import uniffi.bridgething_companion.NoHandle

private class RecordingLinkInbox : LinkInbox(NoHandle) {
    val events = LinkedBlockingQueue<String>()
    val bytes = LinkedBlockingQueue<ByteArray>()

    override fun onConnected(device: LinkDevice) {
        events.add("connected:${device.id}:${device.name}")
    }

    override fun onDisconnected(deviceId: String) {
        events.add("disconnected:$deviceId")
    }

    override fun onLinkFailed(deviceId: String, name: String, reason: String) {
        events.add("linkFailed:$deviceId")
    }

    override fun onBytes(deviceId: String, bytes: ByteArray) {
        this.bytes.add(bytes)
        events.add("bytes:$deviceId")
    }

    override fun onWriteComplete(deviceId: String) {
        events.add("writeComplete:$deviceId")
    }

    override fun onSendFailed(deviceId: String) {
        events.add("sendFailed:$deviceId")
    }
}

class BtLinkTransportTest {
    private val address = "AA:BB:CC:DD:EE:FF"

    private fun rig(): Triple<BtLinkTransport, RecordingLinkInbox, Harness> {
        val toPhone = PipedOutputStream()
        val fromDevice = PipedInputStream(toPhone)
        val written = ByteArrayOutputStream()

        val socket = mockk<BluetoothSocket>(relaxed = true)
        every { socket.inputStream } returns fromDevice
        every { socket.outputStream } returns written

        val device = mockk<BluetoothDevice>(relaxed = true)
        every { device.address } returns address
        every { device.name } returns "thing"
        every { device.bondState } returns BluetoothDevice.BOND_BONDED
        every { device.createInsecureRfcommSocketToServiceRecord(any()) } returns socket

        val transport = BtLinkTransport()
        val inbox = RecordingLinkInbox()
        transport.start(inbox)
        return Triple(transport, inbox, Harness(device, toPhone, written))
    }

    private class Harness(
        val device: BluetoothDevice,
        val toPhone: PipedOutputStream,
        val written: ByteArrayOutputStream,
    )

    @Test
    fun connectReportsTheDeviceAndPumpsInboundBytes() {
        val (transport, inbox, harness) = rig()
        try {
            runBlocking { transport.connect(harness.device) }
            assertEquals("connected:$address:thing", inbox.events.poll(5, TimeUnit.SECONDS))

            harness.toPhone.write(byteArrayOf(1, 2, 3))
            harness.toPhone.flush()
            assertEquals("bytes:$address", inbox.events.poll(5, TimeUnit.SECONDS))
            assertArrayEquals(byteArrayOf(1, 2, 3), inbox.bytes.poll(1, TimeUnit.SECONDS))
        } finally {
            transport.stop()
        }
    }

    @Test
    fun aWriteLandsOnTheSocketAndReturnsTheCredit() {
        val (transport, inbox, harness) = rig()
        try {
            runBlocking { transport.connect(harness.device) }
            assertEquals("connected:$address:thing", inbox.events.poll(5, TimeUnit.SECONDS))

            transport.send(address, byteArrayOf(9, 8, 7))
            assertEquals("writeComplete:$address", inbox.events.poll(5, TimeUnit.SECONDS))
            assertArrayEquals(byteArrayOf(9, 8, 7), harness.written.toByteArray())
        } finally {
            transport.stop()
        }
    }

    @Test
    fun aBatchForADeviceWithNoSessionIsReportedRatherThanDropped() {
        val (transport, inbox, _) = rig()
        try {
            transport.send(address, byteArrayOf(9, 8, 7))
            assertEquals(
                "sendFailed:$address",
                inbox.events.poll(5, TimeUnit.SECONDS),
                "a batch the transport cannot hand over has to be reported: the core is waiting on a write " +
                    "completion for it and never sends again until one arrives",
            )
        } finally {
            transport.stop()
        }
    }

    @Test
    fun aDeadStreamReportsDisconnected() {
        val (transport, inbox, harness) = rig()
        try {
            runBlocking { transport.connect(harness.device) }
            assertEquals("connected:$address:thing", inbox.events.poll(5, TimeUnit.SECONDS))

            harness.toPhone.close()
            assertEquals("disconnected:$address", inbox.events.poll(5, TimeUnit.SECONDS))
        } finally {
            transport.stop()
        }
    }

    @Test
    fun anUnbondedDeviceIsRefusedNotConnected() {
        val transport = BtLinkTransport()
        val inbox = RecordingLinkInbox()
        transport.start(inbox)
        try {
            val device = mockk<BluetoothDevice>(relaxed = true)
            every { device.address } returns address
            every { device.name } returns "thing"
            every { device.bondState } returns BluetoothDevice.BOND_NONE

            val outcome = runCatching { runBlocking { transport.connect(device) } }
            assertTrue(outcome.exceptionOrNull() is BtLinkException.NotBonded)
            assertTrue(inbox.events.isEmpty(), "a refused connect reports nothing")
        } finally {
            transport.stop()
        }
    }

    @Test
    fun forgetClosesTheSessionAndReportsDisconnected() {
        val (transport, inbox, harness) = rig()
        try {
            runBlocking { transport.connect(harness.device) }
            assertEquals("connected:$address:thing", inbox.events.poll(5, TimeUnit.SECONDS))

            runBlocking { transport.forget(address) }
            assertEquals("disconnected:$address", inbox.events.poll(5, TimeUnit.SECONDS))
        } finally {
            transport.stop()
        }
    }
}
