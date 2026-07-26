package com.bridgething.companion

import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayNotificationsMsg
import com.bridgething.schema.DismissReason
import com.bridgething.schema.GatewayToBridgeMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeNotificationsMsg
import com.bridgething.schema.Notification as WireNotification
import com.bridgething.schema.NotificationApp
import com.bridgething.schema.NotificationCategory
import com.bridgething.schema.NotificationFlags
import com.bridgething.schema.NotificationInvoke
import com.bridgething.schema.NotificationRemoved
import io.mockk.mockk
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Test

class NotificationDispatchTest {
    private class RecordingBackend : NotificationBackend {
        val positive = CopyOnWriteArrayList<String>()
        val negative = CopyOnWriteArrayList<String>()
        private val _events = MutableSharedFlow<NotificationOutEvent>(extraBufferCapacity = 64)
        override val events: Flow<NotificationOutEvent> = _events.asSharedFlow()
        override suspend fun invokePositive(id: String) { positive.add(id) }
        override suspend fun invokeNegative(id: String) { negative.add(id) }
        fun emit(event: NotificationOutEvent) { _events.tryEmit(event) }
    }

    private fun wireNotif(id: String) = WireNotification(
        id = id,
        app = NotificationApp(bundleId = "com.example", displayName = "Example", iconAssetId = null),
        category = NotificationCategory.Other,
        title = "Title $id",
        subtitle = null,
        message = "Body",
        timestampUnixS = 0u,
        flags = NotificationFlags(silent = false, important = false),
        positiveAction = null,
        negativeAction = null,
    )

    private fun postedId(msg: GatewayToBridgeMsg): String? =
        ((msg.data as? GatewayToBridgeMsgData.Notifications)?.data as? GatewayToBridgeNotificationsMsg.Posted)?.data?.id

    private suspend fun boot(
        scope: CoroutineScope,
        backend: NotificationBackend,
        caps: CompanionCapabilityFlags = CompanionCapabilityFlags(),
    ): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "notif-test", appVersion = "0.0.1", osName = "test"),
            capabilities = caps,
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
            notifications = backend,
        )
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    @Test
    fun `invoke positive and negative route to the backend with id`() = runBlocking {
        val backend = RecordingBackend()
        val (companion, driver) = boot(this, backend)
        driver.send(BridgeToGatewayMsgData.Notifications(BridgeToGatewayNotificationsMsg.InvokePositive(NotificationInvoke(id = "n-1"))))
        driver.send(BridgeToGatewayMsgData.Notifications(BridgeToGatewayNotificationsMsg.InvokeNegative(NotificationInvoke(id = "n-2"))))
        eventually { backend.positive == listOf("n-1") && backend.negative == listOf("n-2") }
        companion.stop()
    }

    @Test
    fun `a posted event relays to the gateway`() = runBlocking {
        val backend = RecordingBackend()
        val (companion, driver) = boot(this, backend)
        backend.emit(NotificationOutEvent.Posted(wireNotif("p-1")))
        val msg = driver.waitOutbound(20.seconds) { postedId(it) == "p-1" }
        assertEquals("p-1", postedId(msg))
        companion.stop()
    }

    @Test
    fun `a removed event relays to the gateway`() = runBlocking {
        val backend = RecordingBackend()
        val (companion, driver) = boot(this, backend)
        backend.emit(NotificationOutEvent.Removed(NotificationRemoved(id = "r-1", reason = DismissReason.RemoteDismissed)))
        val msg = driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Notifications)?.data is GatewayToBridgeNotificationsMsg.Removed
        }
        val removed = ((msg.data as GatewayToBridgeMsgData.Notifications).data as GatewayToBridgeNotificationsMsg.Removed).data
        assertEquals("r-1", removed.id)
        assertEquals(DismissReason.RemoteDismissed, removed.reason)
        companion.stop()
    }

    @Test
    fun `connect does not backfill the peer with the existing shade`() = runBlocking {
        val backend = RecordingBackend()
        val (companion, driver) = boot(this, backend)
        val backfilled = try {
            driver.waitOutbound(800.milliseconds) { postedId(it) != null }
            true
        } catch (e: TimeoutCancellationException) {
            false
        }
        assertFalse(backfilled, "a connect must post nothing; only notifications posted afterwards are forwarded")
        backend.emit(NotificationOutEvent.Posted(wireNotif("live-1")))
        assertEquals("live-1", postedId(driver.waitOutbound(20.seconds) { postedId(it) == "live-1" }))
        companion.stop()
    }

    @Test
    fun `posted is dropped while the notifications cap is off`() = runBlocking {
        val backend = RecordingBackend()
        val (companion, driver) = boot(this, backend, caps = CompanionCapabilityFlags(notifications = false))
        backend.emit(NotificationOutEvent.Posted(wireNotif("off-1")))
        val arrived = try {
            driver.waitOutbound(800.milliseconds) { postedId(it) == "off-1" }
            true
        } catch (e: TimeoutCancellationException) {
            false
        }
        assertFalse(arrived, "a posted must not be forwarded while the notifications cap is off")
        companion.stop()
    }

    private suspend fun eventually(predicate: () -> Boolean) {
        repeat(300) {
            if (predicate()) return
            delay(10)
        }
        assertEquals(true, predicate(), "predicate did not hold within the deadline")
    }
}
