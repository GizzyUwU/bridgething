package com.bridgething.companion

import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgeTimeMsg
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import kotlin.time.Duration.Companion.seconds

class TimeDispatchTest {
    private suspend fun boot(scope: CoroutineScope): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "time-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
        )
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    @Test
    fun `emits time snapshot on connect`() = runBlocking {
        val (companion, driver) = boot(this)
        val frame = withTimeout(3.seconds) {
            driver.waitOutbound { (it.data as? GatewayToBridgeMsgData.Time)?.data is GatewayToBridgeTimeMsg.Snapshot }
        }
        val info = ((frame.data as GatewayToBridgeMsgData.Time).data as GatewayToBridgeTimeMsg.Snapshot).data
        assertNotNull(info.tzIana)
        assertNotNull(info.locale)
        assertTrue((info.wallClockUnixS?.toLong() ?: 0L) > 1_700_000_000L, "wall clock should be after 2023-11")

        val nowMs = System.currentTimeMillis()
        val tz = java.util.TimeZone.getDefault()
        val dstMinutes = (if (tz.inDaylightTime(java.util.Date(nowMs))) tz.dstSavings else 0) / 60000
        assertEquals(dstMinutes, (info.dstOffsetMinutes ?: 0).toInt())
        assertEquals(
            tz.getOffset(nowMs) / 60000,
            (info.utcOffsetMinutes ?: 0).toInt() + (info.dstOffsetMinutes ?: 0).toInt(),
            "utcOffsetMinutes + dstOffsetMinutes must be the offset from GMT",
        )
        companion.stop()
    }
}
