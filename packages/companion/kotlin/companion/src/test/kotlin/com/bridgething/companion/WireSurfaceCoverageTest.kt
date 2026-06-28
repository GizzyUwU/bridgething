package com.bridgething.companion

import com.bridgething.gateway.WireSurfaceManifest
import com.bridgething.schema.AssetRequest
import com.bridgething.schema.BridgeToGatewayAssetMsg
import com.bridgething.schema.BridgeToGatewayLibraryMsg
import com.bridgething.schema.BridgeToGatewayLyricsMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayPhoneMsg
import com.bridgething.schema.BridgeToGatewaySystemMsg
import com.bridgething.schema.BridgeToGatewayTunnelMsg
import com.bridgething.schema.KeepalivePing
import com.bridgething.schema.TunnelOpen
import com.bridgething.schema.LibraryBrowseRequest
import com.bridgething.schema.LibraryResolveContextRequest
import com.bridgething.schema.LibraryFavoritesContainsRequest
import com.bridgething.schema.LibraryFavoritesListRequest
import com.bridgething.schema.LibraryRecommendationsRequest
import com.bridgething.schema.LibrarySearchRequest
import com.bridgething.schema.LyricsRequest
import com.bridgething.schema.TrackIdentity
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertDoesNotThrow
import java.util.UUID
import kotlin.time.Duration.Companion.seconds

/** coverage ratchet: every inbound request must be probed or classified; every command/event must be accounted for. a new manifest entry fails until consciously classified. */
class WireSurfaceCoverageTest {
    private suspend fun boot(scope: CoroutineScope): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "coverage", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
        )
        companion.setActive(FakeGlue())
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    // a reply of any kind proves the dispatcher exists; absence within the timeout is the silent-hang bug
    private val probes: Map<String, BridgeToGatewayMsgData> = mapOf(
        "asset.request" to BridgeToGatewayMsgData.Asset(BridgeToGatewayAssetMsg.Request(AssetRequest(id = "probe", requestId = UUID.randomUUID()))),
        "library.browse" to BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.Browse(LibraryBrowseRequest(nodeId = null, limit = 1u, offset = 0u))),
        "library.resolveContext" to BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.ResolveContext(LibraryResolveContextRequest(uri = "x"))),
        "library.search" to BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.Search(LibrarySearchRequest(query = "x", kinds = null, limit = 1u, offset = 0u))),
        "library.recommendations" to BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.Recommendations(LibraryRecommendationsRequest(seeds = emptyList(), kind = null, limit = 1u, offset = 0u))),
        "library.favoritesList" to BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.FavoritesList(LibraryFavoritesListRequest(limit = 1u, offset = 0u))),
        "library.favoritesContains" to BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.FavoritesContains(LibraryFavoritesContainsRequest(uris = listOf("x")))),
        "lyrics.get" to BridgeToGatewayMsgData.Lyrics(BridgeToGatewayLyricsMsg.Get(LyricsRequest(track = TrackIdentity(artist = "a", track = "b")))),
        // closed port refuses fast, giving an ErrorReply that proves the dispatcher answers
        "tunnel.open" to BridgeToGatewayMsgData.Tunnel(BridgeToGatewayTunnelMsg.Open(TunnelOpen(tunnelId = UUID.randomUUID(), host = "127.0.0.1", port = 1u))),
        "phone.stateGet" to BridgeToGatewayMsgData.Phone(BridgeToGatewayPhoneMsg.StateGet),
        "system.keepalive" to BridgeToGatewayMsgData.System(BridgeToGatewaySystemMsg.Keepalive(KeepalivePing(seq = 0u))),
    )

    // proven to reply but require real I/O outside this hermetic ratchet
    private val handledElsewhere: Set<String> = setOf(
        "net.fetch",
        "geo.getOnce",
        "net.wsOpen",
        "system.otaAssetRange",
    )

    private val knownUnimplemented: Set<String> = emptySet()

    @Test
    fun `every inbound request is classified`() {
        val accounted = probes.keys + handledElsewhere + knownUnimplemented
        assertEquals(
            WireSurfaceManifest.inboundRequests.toSet(),
            accounted,
            "inbound request surface drift: classify each id as probe / handledElsewhere / knownUnimplemented",
        )
    }

    @Test
    fun `probed requests get a reply and never hang`() = runBlocking {
        val (companion, driver) = boot(this)
        for ((id, data) in probes) {
            assertDoesNotThrow("inbound request `$id` did not reply within the timeout (silent hang)") {
                runBlocking { withTimeout(3.seconds) { driver.request(data, 3.seconds) } }
            }
        }
        companion.stop()
    }

    // fire-and-forget; ratchet is completeness only -- a new id must be consciously listed here
    private val accountedCommandsAndEvents: Set<String> = setOf(
        // player
        "player.play", "player.pause", "player.queue", "player.resume",
        "player.seekTo", "player.setCrossfade", "player.setRepeat", "player.setShuffle",
        "player.setSpeed", "player.skipNext", "player.skipPrev", "player.skipToIndex",
        // library favorites
        "library.favoritesSet", "library.favoritesSetMany", "library.favoritesToggle",
        // geo
        "geo.watch", "geo.unwatch",
        // net
        "net.streamOpen", "net.streamCancel", "net.wsClose", "net.wsSend",
        // notifications
        "notifications.ancsAuthStateChanged", "notifications.invokePositive", "notifications.invokeNegative",
        // audio
        "audio.volumeUp", "audio.volumeDown", "audio.setVolume", "audio.muteToggle",
        "audio.setMute", "audio.tts", "audio.ttsCancel", "audio.ttsCancelAll", "audio.earcon",
        // phone
        "phone.answer", "phone.accept", "phone.decline", "phone.end", "phone.endTyped",
        "phone.hold", "phone.unhold", "phone.initiate", "phone.swap", "phone.merge",
        "phone.mute", "phone.dtmf",
        // tunnel
        "tunnel.data", "tunnel.close",
        // system ota
        "system.otaProgress", "system.otaError", "system.otaBeginAck",
        "system.otaBeginRejected", "system.otaAssetRangeAbandon",
        // system nicknames
        "system.deviceNickname", "system.deviceNicknameChanged", "system.deviceNicknameRejected",
        // system logs
        "system.logEntry", "system.logsTailReply", "system.logsSubscribeReply",
        // voice
        "voice.streamOpen", "voice.frame", "voice.streamClose", "voice.dispatched", "voice.dispatchFailed",
        // webapp
        "webapp.webapps", "webapp.active", "webapp.switched", "webapp.uninstalled",
        "webapp.webappError", "webapp.icon", "webapp.configGet",
        "webapp.configList", "webapp.configAck", "webapp.webappInstalled", "webapp.activeChanged",
        // forward
        "forward.text", "forward.binary", "forward.json",
    )

    @Test
    fun `every inbound command or event is accounted for`() {
        val manifest = WireSurfaceManifest.inboundCommands.toSet() + WireSurfaceManifest.inboundEvents.toSet()
        assertEquals(
            manifest,
            accountedCommandsAndEvents,
            "inbound command/event surface drift: a new variant appeared (or one was removed) - classify it",
        )
    }
}
