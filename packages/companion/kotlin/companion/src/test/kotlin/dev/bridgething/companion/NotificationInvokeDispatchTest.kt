package dev.bridgething.companion

import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.BridgeToGatewayNotificationsMsg
import dev.bridgething.schema.NotificationInvoke
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import java.util.concurrent.CopyOnWriteArrayList

/** notification invoke dispatch: verifies positive and negative invoke verbs reach the [NotificationActionBackend] with the correct id. */
class NotificationInvokeDispatchTest {
    private suspend fun boot(scope: CoroutineScope, backend: NotificationActionBackend): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "notif-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
            notificationActions = backend,
        )
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    @Test
    fun `invoke positive and negative route to backend with id`() = runBlocking {
        val backend = RecordingNotificationActionBackend()
        val (companion, driver) = boot(this, backend)
        driver.send(BridgeToGatewayMsgData.Notifications(BridgeToGatewayNotificationsMsg.InvokePositive(NotificationInvoke(id = "n-1"))))
        driver.send(BridgeToGatewayMsgData.Notifications(BridgeToGatewayNotificationsMsg.InvokeNegative(NotificationInvoke(id = "n-2"))))
        eventually { backend.positive == listOf("n-1") && backend.negative == listOf("n-2") }
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

class RecordingNotificationActionBackend : NotificationActionBackend {
    val positive = CopyOnWriteArrayList<String>()
    val negative = CopyOnWriteArrayList<String>()
    override suspend fun invokePositive(id: String) { positive.add(id) }
    override suspend fun invokeNegative(id: String) { negative.add(id) }
}
