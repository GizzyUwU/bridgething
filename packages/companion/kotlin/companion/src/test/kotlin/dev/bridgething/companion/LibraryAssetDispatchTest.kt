package dev.bridgething.companion

import dev.bridgething.glue.AssetBytes
import dev.bridgething.lyrics.LyricLine
import dev.bridgething.lyrics.Lyrics as DomainLyrics
import dev.bridgething.schema.AssetRequest
import dev.bridgething.schema.BridgeToGatewayAssetMsg
import dev.bridgething.schema.BridgeToGatewayLibraryMsg
import dev.bridgething.schema.BridgeToGatewayLyricsMsg
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.FavoritesPage
import dev.bridgething.schema.GatewayToBridgeAssetMsg
import dev.bridgething.schema.GatewayToBridgeCapabilitiesMsg
import dev.bridgething.schema.GatewayToBridgeLibraryMsg
import dev.bridgething.schema.GatewayToBridgeLyricsMsg
import dev.bridgething.schema.GatewayToBridgeMsgData
import dev.bridgething.schema.ItemKind
import dev.bridgething.schema.LibraryFavoritesListRequest
import dev.bridgething.schema.LibrarySearchRequest
import dev.bridgething.schema.LyricsRequest
import dev.bridgething.schema.MusicProvider
import dev.bridgething.schema.SearchResult
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.UUID

/** breadth coverage for asset, lyrics, library, and outbound capabilities dispatch. */
class LibraryAssetDispatchTest {
    private suspend fun boot(
        scope: CoroutineScope,
        glue: FakeGlue?,
        lyricsResolver: FakeLyricsResolver = FakeLyricsResolver(),
    ): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = lyricsResolver,
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

    // MARK: - asset

    @Test
    fun `asset request resolves to glue bytes`() = runBlocking {
        val payload = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)
        val glue = FakeGlue(onAsset = { id ->
            assertEquals("art:track:1", id)
            AssetBytes(bytes = payload, mime = "image/png")
        })
        val (companion, driver) = boot(this, glue)

        val resp = driver.request(BridgeToGatewayMsgData.Asset(BridgeToGatewayAssetMsg.Request(AssetRequest(id = "art:track:1", requestId = UUID.randomUUID()))))
        val asset = resp.data as GatewayToBridgeMsgData.Asset
        val got = asset.data as GatewayToBridgeAssetMsg.Got
        assertEquals("art:track:1", got.data.id)
        assertTrue(payload.contentEquals(got.data.bytes))
        assertEquals("image/png", got.data.mime)
        assertTrue(glue.calls.contains("asset:art:track:1"))

        companion.stop()
    }

    @Test
    fun `asset miss returns notFound`() = runBlocking {
        // fakeglue with no onAsset returns null -> notFound, not a hang
        val (companion, driver) = boot(this, FakeGlue())

        val resp = driver.request(BridgeToGatewayMsgData.Asset(BridgeToGatewayAssetMsg.Request(AssetRequest(id = "art:missing", requestId = UUID.randomUUID()))))
        val asset = resp.data as GatewayToBridgeMsgData.Asset
        val notFound = asset.data as GatewayToBridgeAssetMsg.NotFound
        assertEquals("art:missing", notFound.data.id)

        companion.stop()
    }

    // MARK: - lyrics

    @Test
    fun `lyrics falls through to resolver`() = runBlocking {
        val canned = DomainLyrics(synced = listOf(LyricLine(0, "one more time")), plain = null, source = "fake-resolver")
        val (companion, driver) = boot(this, FakeGlue(), FakeLyricsResolver(canned))

        val resp = driver.request(
            BridgeToGatewayMsgData.Lyrics(
                BridgeToGatewayLyricsMsg.Get(LyricsRequest(track = dev.bridgething.schema.TrackIdentity(artist = "Daft Punk", track = "One More Time"))),
            ),
        )
        val lyrics = resp.data as GatewayToBridgeMsgData.Lyrics
        val reply = lyrics.data as GatewayToBridgeLyricsMsg.LyricsReply
        assertEquals("fake-resolver", reply.data.lyrics?.source)
        assertEquals("one more time", reply.data.lyrics?.synced?.first()?.text)

        companion.stop()
    }

    @Test
    fun `lyrics no hit returns nil reply`() = runBlocking {
        val (companion, driver) = boot(this, FakeGlue(), FakeLyricsResolver(null))

        val resp = driver.request(
            BridgeToGatewayMsgData.Lyrics(
                BridgeToGatewayLyricsMsg.Get(LyricsRequest(track = dev.bridgething.schema.TrackIdentity(artist = "Unknown", track = "Nope"))),
            ),
        )
        val lyrics = resp.data as GatewayToBridgeMsgData.Lyrics
        val reply = lyrics.data as GatewayToBridgeLyricsMsg.LyricsReply
        assertNull(reply.data.lyrics)

        companion.stop()
    }

    // MARK: - library breadth

    @Test
    fun `search routes to the active glue`() = runBlocking {
        val glue = FakeGlue(onSearch = { req ->
            assertEquals("daft punk", req.query)
            SearchResult(items = emptyList(), kinds = listOf(ItemKind.Track), total = 0u, hasMore = false)
        })
        val (companion, driver) = boot(this, glue)

        val resp = driver.request(
            BridgeToGatewayMsgData.Library(
                BridgeToGatewayLibraryMsg.Search(LibrarySearchRequest(query = "daft punk", kinds = null, limit = 10u, offset = 0u)),
            ),
        )
        val lib = resp.data as GatewayToBridgeMsgData.Library
        val reply = lib.data as GatewayToBridgeLibraryMsg.SearchReply
        assertEquals(listOf(ItemKind.Track), reply.data.result.kinds)
        assertTrue(glue.calls.contains("search:daft punk"))

        companion.stop()
    }

    @Test
    fun `favoritesList routes to the active glue`() = runBlocking {
        val glue = FakeGlue(onFavoritesList = { FavoritesPage(items = emptyList(), total = 42u, hasMore = true) })
        val (companion, driver) = boot(this, glue)

        val resp = driver.request(
            BridgeToGatewayMsgData.Library(
                BridgeToGatewayLibraryMsg.FavoritesList(LibraryFavoritesListRequest(limit = 20u, offset = 0u)),
            ),
        )
        val lib = resp.data as GatewayToBridgeMsgData.Library
        val reply = lib.data as GatewayToBridgeLibraryMsg.FavoritesListReply
        assertEquals(42u, reply.data.page.total)
        assertTrue(reply.data.page.hasMore)
        assertTrue(glue.calls.contains("favoritesList"))

        companion.stop()
    }

    // MARK: - outbound

    @Test
    fun `companion announces capabilities on connect`() = runBlocking {
        val (companion, driver) = boot(this, FakeGlue())

        val frame = driver.waitOutbound { msg ->
            (msg.data as? GatewayToBridgeMsgData.Capabilities)?.data is GatewayToBridgeCapabilitiesMsg.Announce
        }
        val caps = (frame.data as GatewayToBridgeMsgData.Capabilities).data as GatewayToBridgeCapabilitiesMsg.Announce
        assertEquals("test", caps.data.gateway.appName)
        assertEquals(MusicProvider.None, caps.data.musicProvider)

        companion.stop()
    }
}
