package com.bridgething.companion

import com.bridgething.glue.AssetBytes
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayPlayerMsg
import com.bridgething.schema.GatewayToBridgeCapabilitiesMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.PlayUri
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class MultiProviderRoutingTest {
    private suspend fun boot(
        scope: CoroutineScope,
        first: FakeGlue,
        second: FakeGlue,
    ): Pair<BridgethingCompanion, WireDriver> {
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
        companion.attach(first)
        companion.attach(second)
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    @Test
    fun announcesTheUnionOfAttachedSchemes() = runBlocking {
        val first = FakeGlue(name = "alpha", uriSchemes = listOf("alpha"))
        val second = FakeGlue(name = "beta", uriSchemes = listOf("beta"))
        val (companion, driver) = boot(this, first, second)
        val frame = driver.waitOutbound { msg ->
            val announce = (msg.data as? GatewayToBridgeMsgData.Capabilities)?.data
                as? GatewayToBridgeCapabilitiesMsg.Announce
            announce != null && announce.data.uriSchemes.size == 2
        }
        val caps = (frame.data as GatewayToBridgeMsgData.Capabilities).data as GatewayToBridgeCapabilitiesMsg.Announce
        assertEquals(setOf("alpha", "beta"), caps.data.uriSchemes.toSet())
        companion.stop()
    }

    @Test
    fun playRoutesByUriScheme() = runBlocking {
        val first = FakeGlue(name = "alpha", uriSchemes = listOf("alpha"))
        val second = FakeGlue(name = "beta", uriSchemes = listOf("beta"))
        val (companion, driver) = boot(this, first, second)
        driver.send(
            BridgeToGatewayMsgData.Player(
                BridgeToGatewayPlayerMsg.Play(PlayUri(uri = "beta:track:xyz", context = null)),
            ),
        )
        delay(300)
        assertTrue(second.calls.contains("play:beta:track:xyz"))
        assertFalse(first.calls.any { it.startsWith("play:") })
        companion.stop()
    }

    @Test
    fun playForAnUnclaimedSchemeIsDroppedRatherThanMisrouted() = runBlocking {
        val first = FakeGlue(name = "alpha", uriSchemes = listOf("alpha"))
        val second = FakeGlue(name = "beta", uriSchemes = listOf("beta"))
        val (companion, driver) = boot(this, first, second)
        driver.send(
            BridgeToGatewayMsgData.Player(
                BridgeToGatewayPlayerMsg.Play(PlayUri(uri = "tidal:track:xyz", context = null)),
            ),
        )
        delay(300)
        assertFalse(first.calls.any { it.startsWith("play:") })
        assertFalse(second.calls.any { it.startsWith("play:") })
        companion.stop()
    }

    @Test
    fun assetResolvesFromTheGlueThatMintedTheId() = runBlocking {
        val first = FakeGlue(
            name = "alpha",
            uriSchemes = listOf("alpha"),
            onAsset = { id -> if (id.startsWith("alpha/")) AssetBytes("art".toByteArray(), "image/jpeg") else null },
        )
        val second = FakeGlue(name = "beta", uriSchemes = listOf("beta"))
        val (companion, _) = boot(this, first, second)
        assertTrue(companion.attachedProviderIds().containsAll(listOf("alpha", "beta")))
        companion.stop()
    }

    @Test
    fun onlyOneGlueIsAllowedToAutoResumeOnConnect() = runBlocking {
        val first = FakeGlue(name = "alpha", uriSchemes = listOf("alpha"))
        val second = FakeGlue(name = "beta", uriSchemes = listOf("beta"))
        val (companion, _) = boot(this, first, second)
        delay(400)
        val firstAllowed = first.calls.contains("peerConnected:true")
        val secondAllowed = second.calls.contains("peerConnected:true")
        assertFalse(firstAllowed && secondAllowed, "only one provider may resume on connect")
        companion.stop()
    }

    @Test
    fun libraryGlueFollowsProviderPriority() = runBlocking {
        val first = FakeGlue(name = "alpha", uriSchemes = listOf("alpha"))
        val second = FakeGlue(name = "beta", uriSchemes = listOf("beta"))
        val (companion, _) = boot(this, first, second)
        companion.setProviderPriority(listOf("beta", "alpha"))
        assertEquals("beta", companion.libraryGlue()?.name)
        companion.setProviderPriority(listOf("alpha", "beta"))
        assertEquals("alpha", companion.libraryGlue()?.name)
        companion.stop()
    }
}
