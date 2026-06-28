package com.bridgething.companion

import com.bridgething.schema.BridgeToGatewayLibraryMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayPlayerMsg
import com.bridgething.schema.BrowseResult
import com.bridgething.schema.GatewayToBridgeLibraryMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.LibraryBrowseRequest
import com.bridgething.schema.WireError
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import kotlin.time.Duration.Companion.seconds

/** companion dispatch: drives wire requests through [BridgethingCompanion] + [FakeGlue] at the [FakeAdapter] seam and asserts response frames. */
class CompanionDispatchTest {
    private suspend fun boot(scope: CoroutineScope, glue: FakeGlue?): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
        )
        if (glue != null) companion.setActive(glue)
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    private fun browseReq() = BridgeToGatewayMsgData.Library(
        BridgeToGatewayLibraryMsg.Browse(LibraryBrowseRequest(nodeId = null, limit = 20u, offset = 0u)),
    )

    @Test
    fun `companion logs land in the device log ring`() = runBlocking {
        val (companion, _) = boot(this, null)
        val logged = DeviceLogRing.tail(512).any { it.message == "companion started" }
        assertTrue(logged)
        companion.stop()
    }

    @Test
    fun `browse routes to the active glue and returns its result`() = runBlocking {
        val glue = FakeGlue(onBrowse = { BrowseResult(entries = emptyList(), total = 7u, hasMore = false) })
        val (companion, driver) = boot(this, glue)

        val resp = driver.request(browseReq())
        val lib = resp.data as GatewayToBridgeMsgData.Library
        val reply = lib.data as GatewayToBridgeLibraryMsg.BrowseReply
        assertEquals(7u, reply.data.result.total)
        assertTrue(glue.calls.contains("browse"))

        companion.stop()
    }

    @Test
    fun `library request with no active glue returns a LibraryError`() = runBlocking {
        val (companion, driver) = boot(this, null)

        val resp = driver.request(browseReq())
        val lib = resp.data as GatewayToBridgeMsgData.Library
        assertTrue(lib.data is GatewayToBridgeLibraryMsg.LibraryErrorReply)

        companion.stop()
    }

    @Test
    fun `unimplemented library verb maps to a protocol Unimplemented error`() = runBlocking {
        val (companion, driver) = boot(this, FakeGlue(onBrowse = null))

        val resp = driver.request(browseReq())
        val err = resp.data as GatewayToBridgeMsgData.Error
        assertTrue(err.data is WireError.Unimplemented)

        companion.stop()
    }

    @Test
    fun `player command reaches the glue`() = runBlocking {
        val glue = FakeGlue()
        val (companion, driver) = boot(this, glue)

        driver.send(BridgeToGatewayMsgData.Player(BridgeToGatewayPlayerMsg.Pause))
        withTimeout(2.seconds) { while (!glue.calls.contains("pause")) delay(10) }

        companion.stop()
    }
}
