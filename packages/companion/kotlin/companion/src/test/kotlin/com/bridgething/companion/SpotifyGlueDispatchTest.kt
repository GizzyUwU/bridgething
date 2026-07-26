package com.bridgething.companion

import com.bridgething.schema.BridgeToGatewayAudioMsg
import com.bridgething.schema.BridgeToGatewayLibraryMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayPlayerMsg
import com.bridgething.schema.SkipToIndex
import com.bridgething.schema.GatewayToBridgeAudioMsg
import com.bridgething.schema.GatewayToBridgeAuthorityMsg
import com.bridgething.schema.GatewayToBridgeLibraryMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgePlayerMsg
import com.bridgething.schema.ItemKind
import com.bridgething.schema.LibraryBrowseRequest
import com.bridgething.schema.LibraryItem
import com.bridgething.schema.LibraryScope as WireLibraryScope
import com.bridgething.schema.LibrarySearchRequest
import com.bridgething.schema.PlaybackState
import com.bridgething.schema.PlaybackTargetKind
import com.bridgething.schema.TransferTo as WireTransferTo
import com.bridgething.schema.BrowseEntry
import com.bridgething.schema.CompanionAuthorityScope
import com.bridgething.glue.GlueAuthState
import com.bridgething.spotify.ConnectivityWatcher
import com.bridgething.spotify.NoOpConnectivityWatcher
import com.bridgething.spotify.SpotifyGlue
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import kotlin.time.Duration.Companion.seconds
import uniffi.spotify.AuthState as SpAuthState
import uniffi.spotify.Album as SpAlbum
import uniffi.spotify.Artist as SpArtist
import uniffi.spotify.BrowseItem as SpBrowseItem
import uniffi.spotify.BrowsePage as SpBrowsePage
import uniffi.spotify.Device as SpDevice
import uniffi.spotify.DeviceFlow as SpDeviceFlow
import uniffi.spotify.DeviceKind as SpDeviceKind
import uniffi.spotify.LibraryScope as SpLibraryScope
import uniffi.spotify.Observer as SpObserver
import uniffi.spotify.PlayerState as SpPlayerState
import uniffi.spotify.ProductState as SpProductState
import uniffi.spotify.Queue as SpQueue
import uniffi.spotify.RepeatMode as SpRepeat
import uniffi.spotify.SearchResults as SpSearchResults
import uniffi.spotify.Shelf as SpShelf
import uniffi.spotify.SpotifyClientInterface
import uniffi.spotify.Track as SpTrack
import uniffi.spotify.TokenStore as SpTokenStore

class SpotifyGlueDispatchTest {
    private class FakeTokenStore(private var refresh: String?) : SpTokenStore {
        override fun loadRefreshToken(): String? = refresh
        override fun saveRefreshToken(token: String) { refresh = token }
        override fun loadUsername(): String? = null
        override fun saveUsername(username: String) {}
    }

    private class FakeClient : SpotifyClientInterface {
        var root: List<SpShelf> = emptyList()
        var page = SpBrowsePage(items = emptyList(), total = 0u, hasMore = false)
        var searchResults = SpSearchResults(tracks = emptyList(), albums = emptyList(), artists = emptyList(), playlists = emptyList())
        var contains: List<Boolean> = emptyList()
        var productState = SpProductState(product = "premium", catalogue = "premium", country = "US", isPremium = true, canUseSuperbird = true)
        var observer: SpObserver? = null
        var onConnect: (suspend (SpObserver) -> Unit)? = null
        var favoritesContainsCalls = 0
        @Volatile var lastPlay: Pair<String, String?>? = null
        @Volatile var currentPosition: UInt? = null
        @Volatile var resyncCalls = 0

        override suspend fun connect() { observer?.let { onConnect?.invoke(it) } }
        override suspend fun disconnect() {}
        override suspend fun resync() { resyncCalls++ }
        override suspend fun currentPositionMs(): UInt? = currentPosition
        override fun setWsTransport(transport: uniffi.spotify.WsTransport) {}
        override fun setHttpTransport(transport: uniffi.spotify.HttpTransport) {}
        override fun setDeviceWaker(waker: uniffi.spotify.DeviceWaker) {}
        override suspend fun pause() {}
        @Volatile var resumeCalls = 0
        override suspend fun resume() { resumeCalls++ }
        @Volatile var resumeOnConnectCalls = 0
        override suspend fun resumeOnConnect() { resumeOnConnectCalls++ }
        override suspend fun skipNext() {}
        override suspend fun skipPrev() {}
        override suspend fun seek(positionMs: Long) {}
        override suspend fun setShuffle(on: Boolean) {}
        override suspend fun setRepeat(mode: SpRepeat) {}
        var volume: Double = 50.0
        val volumeSets = mutableListOf<Double>()
        override suspend fun setVolume(percent: Double) {
            volume = percent
            volumeSets.add(percent)
        }
        override suspend fun volumeStep(deltaPercent: Double): Double {
            volume = (volume + deltaPercent).coerceIn(0.0, 100.0)
            volumeSets.add(volume)
            return volume
        }
        override suspend fun activeDeviceVolumePercent(): Double? = volume
        override suspend fun queueUri(uri: String) {}
        val transferCalls = java.util.concurrent.CopyOnWriteArrayList<String>()
        override suspend fun transfer(deviceId: String) { transferCalls.add(deviceId) }
        override suspend fun play(uri: String, skipToUri: String?) { lastPlay = uri to skipToUri }
        override suspend fun product(): SpProductState = productState
        @Volatile var lastRootBrowse: Pair<UInt?, UInt?>? = null
        override suspend fun rootBrowse(sections: UInt?, preview: UInt?): List<SpShelf> {
            lastRootBrowse = sections to preview
            return root
        }
        override suspend fun browse(nodeId: String, limit: UInt, offset: UInt): SpBrowsePage = page
        override suspend fun search(query: String, limit: UInt): SpSearchResults = searchResults
        override suspend fun resolveContext(uri: String): SpBrowseItem = item("spotify:playlist:1", "Ctx")
        override suspend fun favoritesContains(uris: List<String>): List<Boolean> { favoritesContainsCalls++; return contains }
        override suspend fun favoritesSet(uri: String, liked: Boolean) {}
        override suspend fun favoritesList(limit: UInt, offset: UInt): SpBrowsePage = page
        override suspend fun beginDeviceFlow(): SpDeviceFlow =
            SpDeviceFlow(deviceCode = "dc", userCode = "ABCD", verificationUri = "https://spotify.com/pair", interval = 1u, expiresIn = 60u)
        override suspend fun completeDeviceFlow(flow: SpDeviceFlow) {}
    }

    private class FakeConnectivityWatcher : ConnectivityWatcher {
        private var callback: ((Boolean) -> Unit)? = null
        override fun start(onAvailability: (Boolean) -> Unit) { callback = onAvailability }
        override fun stop() { callback = null }
        fun emit(available: Boolean) { callback?.invoke(available) }
    }

    private class Harness(
        val companion: BridgethingCompanion,
        val driver: WireDriver,
        val fake: FakeClient,
        val glue: SpotifyGlue,
        val observer: () -> SpObserver?,
    )

    private suspend fun boot(
        scope: CoroutineScope,
        fake: FakeClient,
        connectivity: ConnectivityWatcher = NoOpConnectivityWatcher,
        autoResume: Boolean? = false,
        authSink: ((GlueAuthState) -> Unit)? = null,
    ): Harness {
        val glue = SpotifyGlue(
            workerBase = "https://example/auth",
            psk = "psk",
            deviceId = "dev",
            tokenStore = FakeTokenStore("rt"),
            cacheDir = null,
            connectivity = connectivity,
            clientFactory = { _, obs -> fake.observer = obs; fake },
        )
        if (authSink != null) glue.setAuthObserver(authSink)
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "spotify-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
        )
        companion.attach(glue)
        companion.start()
        if (autoResume != null) companion.setDeviceAutoResume("carthing-test", autoResume)
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return Harness(companion, driver, fake, glue) { fake.observer }
    }

    // MARK: - library mapping

    @Test
    fun `browse root maps shelves to folders`() = runBlocking {
        val fake = FakeClient()
        fake.root = listOf(
            SpShelf(id = "playlists", title = "Playlists", items = listOf(item("spotify:playlist:1", "Mix", hasChildren = true)), total = 12u),
            SpShelf(id = "albums", title = "Albums", items = listOf(item("spotify:album:1", "Album")), total = 3u),
        )
        val h = boot(this, fake)
        val resp = h.driver.request(
            BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.Browse(LibraryBrowseRequest(nodeId = null, limit = 20u, offset = 0u))),
        )
        val reply = (resp.data as GatewayToBridgeMsgData.Library).data as GatewayToBridgeLibraryMsg.BrowseReply
        assertEquals(2, reply.data.result.entries.size)
        val folder = (reply.data.result.entries.first() as BrowseEntry.Folder).data
        assertEquals("playlists", folder.nodeId)
        assertEquals("Playlists", folder.title)
        assertEquals(1, folder.previewChildren?.size)
        assertEquals(12u, folder.total, "folder total is the shelf's real total, not the preview count")
        h.companion.stop()
    }

    @Test
    fun `browse root forwards sections and preview caps`() = runBlocking {
        val fake = FakeClient()
        fake.root = listOf(SpShelf(id = "playlists", title = "Playlists", items = emptyList(), total = 12u))
        val h = boot(this, fake)
        h.driver.request(
            BridgeToGatewayMsgData.Library(
                BridgeToGatewayLibraryMsg.Browse(LibraryBrowseRequest(nodeId = null, limit = 20u, offset = 0u, sections = 10u, preview = 0u)),
            ),
        )
        assertEquals(10u to 0u, fake.lastRootBrowse)
        h.companion.stop()
    }

    @Test
    fun `browse drill-in maps items by kind`() = runBlocking {
        val fake = FakeClient()
        fake.page = SpBrowsePage(items = listOf(track("spotify:track:1", "Song"), item("spotify:album:9", "Alb")), total = 2u, hasMore = false)
        val h = boot(this, fake)
        val resp = h.driver.request(
            BridgeToGatewayMsgData.Library(BridgeToGatewayLibraryMsg.Browse(LibraryBrowseRequest(nodeId = "albums", limit = 20u, offset = 0u))),
        )
        val reply = (resp.data as GatewayToBridgeMsgData.Library).data as GatewayToBridgeLibraryMsg.BrowseReply
        assertEquals(2, reply.data.result.entries.size)
        val first = (reply.data.result.entries.first() as BrowseEntry.Item).data as LibraryItem.Track
        assertEquals("Song", first.data.name)
        assertFalse(first.data.imageId.isEmpty(), "track art id should be wrapped")
        h.companion.stop()
    }

    @Test
    fun `search maps by requested kinds`() = runBlocking {
        val fake = FakeClient()
        fake.searchResults = SpSearchResults(
            tracks = listOf(track("spotify:track:1", "T")),
            albums = listOf(item("spotify:album:1", "A")),
            artists = emptyList(), playlists = emptyList(),
        )
        val h = boot(this, fake)
        val resp = h.driver.request(
            BridgeToGatewayMsgData.Library(
                BridgeToGatewayLibraryMsg.Search(LibrarySearchRequest(query = "x", kinds = listOf(ItemKind.Track, ItemKind.Album), limit = 10u, offset = 0u)),
            ),
        )
        val reply = (resp.data as GatewayToBridgeMsgData.Library).data as GatewayToBridgeLibraryMsg.SearchReply
        assertEquals(2, reply.data.result.items.size)
        assertEquals(listOf(ItemKind.Track, ItemKind.Album), reply.data.result.kinds)
        h.companion.stop()
    }

    // MARK: - now-playing + authority

    @Test
    fun `player push snapshots and claims authority`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song")))

        val snap = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        val ps = ((snap.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertEquals("Song", ps.track?.title)
        assertEquals(PlaybackState.Playing, ps.playback.state)

        val claim = h.driver.waitOutbound(20.seconds) {
            val c = (it.data as? GatewayToBridgeMsgData.Authority)?.data as? GatewayToBridgeAuthorityMsg.Claim
            c?.data?.scope == CompanionAuthorityScope.NowPlayingPlayback
        }
        val c = ((claim.data as GatewayToBridgeMsgData.Authority).data as GatewayToBridgeAuthorityMsg.Claim).data
        assertEquals("com.spotify.client", c.appBundle)
        h.companion.stop()
    }

    @Test
    fun `peer reconnect replays the fresh position not the stale zero`() = runBlocking {
        val fake = FakeClient()
        fake.currentPosition = 90_000u
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song")))
        val first = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        val stale = ((first.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertEquals(0u, stale.playback.positionMs, "the cached now-playing position is frozen at the last dealer event")

        h.glue.handlePeerConnected(allowAutoResume = false)
        val replay = h.driver.waitOutbound(20.seconds) {
            val d = (it.data as? GatewayToBridgeMsgData.Player)?.data as? GatewayToBridgePlayerMsg.Snapshot
            d?.data?.playback?.positionMs == 90_000u
        }
        val ps = ((replay.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertEquals(90_000u, ps.playback.positionMs, "peer-connect replay must refresh the stale cached position")
        h.companion.stop()
    }

    @Test
    fun `peer reconnect without a fresh position stamps the cached age`() = runBlocking {
        val fake = FakeClient() // currentPosition stays null, so the cached replay cannot be freshened
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song")))
        val first = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        val fresh = ((first.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertNull(fresh.playback.positionAgeMs, "a live dealer emit carries no age")

        h.glue.handlePeerConnected(allowAutoResume = false)
        val replay = h.driver.waitOutbound(20.seconds) {
            val d = (it.data as? GatewayToBridgeMsgData.Player)?.data as? GatewayToBridgePlayerMsg.Snapshot
            d?.data?.playback?.positionAgeMs != null
        }
        val ps = ((replay.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertNotNull(ps.playback.positionAgeMs, "a cached replay that could not be freshened stamps its age")
        h.companion.stop()
    }

    @Test
    fun `aggressive connect runs connect resume`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)

        h.glue.handlePeerConnected(allowAutoResume = true)
        waitFor("connect resume") { fake.resumeOnConnectCalls == 1 }
        assertEquals(0, fake.resumeCalls, "the user resume path is never the connect trigger")
        h.companion.stop()
    }

    @Test
    fun `non-aggressive connect never resumes`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)

        h.glue.handlePeerConnected(allowAutoResume = false)
        delay(500)
        assertEquals(0, fake.resumeOnConnectCalls, "non-aggressive connect must not reconcile playback")
        assertEquals(0, fake.resumeCalls, "non-aggressive connect must not resume")
        h.companion.stop()
    }

    @Test
    fun `companion connect defaults to aggressive resume`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake, autoResume = null)
        waitFor("companion-driven connect resume") { fake.resumeOnConnectCalls == 1 }
        h.companion.stop()
    }

    private suspend fun waitFor(what: String, cond: () -> Boolean) {
        val deadline = System.currentTimeMillis() + 10_000
        while (!cond()) {
            check(System.currentTimeMillis() < deadline) { "timed out waiting for $what" }
            delay(50)
        }
    }

    @Test
    fun `android claims authority even when casting off-phone`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song"), remote = true))

        val claim = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Authority)?.data is GatewayToBridgeAuthorityMsg.Claim
        }
        assertTrue((claim.data as GatewayToBridgeMsgData.Authority).data is GatewayToBridgeAuthorityMsg.Claim)
        h.companion.stop()
    }

    @Test
    fun `volume verbs route to the remote connect device while casting off-phone`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song"), remote = true))
        h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Authority)?.data is GatewayToBridgeAuthorityMsg.Claim
        }
        assertTrue(h.glue.ownsVolume(), "remote playback must own volume")

        h.driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.VolumeUp))
        val changed = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Audio)?.data is GatewayToBridgeAudioMsg.VolumeChanged
        }
        val vol = ((changed.data as GatewayToBridgeMsgData.Audio).data as GatewayToBridgeAudioMsg.VolumeChanged).data
        assertEquals(0.5625f, vol.level, 0.001f, "volumeUp must step the remote connect device")
        assertEquals(56.25, fake.volumeSets.last(), 0.01)
        h.companion.stop()
    }

    @Test
    fun `device push sends targetsChanged`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onDevices(
            listOf(
                device("speaker", "Kitchen", SpDeviceKind.SPEAKER, active = true, volume = 0.4f),
                device("laptop", "Desk", SpDeviceKind.COMPUTER, active = false, volume = 0f),
            ),
        )

        val frame = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.TargetsChanged
        }
        val targets =
            ((frame.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.TargetsChanged).data.targets
        assertEquals(listOf("speaker", "laptop"), targets.map { it.id })
        assertEquals(PlaybackTargetKind.Speaker, targets[0].kind, "the protobuf device type maps to a closed wire kind")
        assertEquals(40u, targets[0].volumePercent)
        assertTrue(targets[0].isActive)
        assertNull(targets[1].volumePercent, "an endpoint reporting no volume must stay null, not zero")
        h.companion.stop()
    }

    @Test
    fun `remote playback names the active target on the snapshot`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onDevices(listOf(device("speaker", "Kitchen", SpDeviceKind.SPEAKER, active = true, volume = 0.4f)))
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song"), remote = true))

        val frame = h.driver.waitOutbound(20.seconds) {
            val snap = (it.data as? GatewayToBridgeMsgData.Player)?.data as? GatewayToBridgePlayerMsg.Snapshot
            snap?.data?.target != null
        }
        val snap = ((frame.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertEquals("speaker", snap.target?.id)
        assertEquals("Kitchen", snap.target?.name, "the readout resolves the name off the cached cluster list")
        h.companion.stop()
    }

    @Test
    fun `local playback leaves the target unset`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onDevices(listOf(device("speaker", "Kitchen", SpDeviceKind.SPEAKER, active = false, volume = 0.4f)))
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song"), remote = false))

        val frame = h.driver.waitOutbound(20.seconds) {
            val snap = (it.data as? GatewayToBridgeMsgData.Player)?.data as? GatewayToBridgePlayerMsg.Snapshot
            snap?.data?.track != null
        }
        val snap = ((frame.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertNull(snap.target, "playing on the phone itself is not a remote endpoint")
        h.companion.stop()
    }

    @Test
    fun `transferTo forwards to the client`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)

        h.driver.send(
            BridgeToGatewayMsgData.Player(BridgeToGatewayPlayerMsg.TransferTo(WireTransferTo(targetId = "speaker"))),
        )
        waitFor("transfer") { fake.transferCalls == listOf("speaker") }
        h.companion.stop()
    }

    @Test
    fun `queue push sends queueChanged`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onQueue(SpQueue(previous = emptyList(), current = null, next = listOf(npTrack("spotify:track:2", "Next"))))

        val q = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.QueueChanged
        }
        val snap = ((q.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.QueueChanged).data
        assertEquals(listOf("spotify:track:2"), snap.order)
        assertEquals("Next", snap.items.first().title)
        h.companion.stop()
    }

    @Test
    fun `peer reconnect re-syncs the held queue even with no now-playing track`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onQueue(SpQueue(previous = emptyList(), current = null, next = listOf(npTrack("spotify:track:2", "Next"))))
        h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.QueueChanged
        }

        h.glue.handlePeerConnected(allowAutoResume = false)
        val resent = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.QueueChanged
        }
        val snap = ((resent.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.QueueChanged).data
        assertEquals(listOf("spotify:track:2"), snap.order, "reconnect must re-sync the held queue even with no now-playing track")
        h.companion.stop()
    }

    @Test
    fun `connectivity restored edge invokes resync exactly once`() = runBlocking {
        val fake = FakeClient()
        val conn = FakeConnectivityWatcher()
        val h = boot(this, fake, connectivity = conn)
        conn.emit(true) // initial already-available callback must not resync
        conn.emit(false)
        conn.emit(true) // lost -> available edge resyncs once

        withTimeout(5.seconds) { while (fake.resyncCalls == 0) delay(10) }
        delay(50)
        assertEquals(1, fake.resyncCalls, "only the connectivity-restored edge should resync, not the initial callback")
        h.companion.stop()
    }

    @Test
    fun `skipToIndex replays context and skips to the queued uri`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song")))
        h.observer()!!.onQueue(
            SpQueue(
                previous = emptyList(), current = null,
                next = listOf(npTrack("spotify:track:2", "Next"), npTrack("spotify:track:3", "After")),
            ),
        )
        h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.QueueChanged
        }

        h.driver.send(BridgeToGatewayMsgData.Player(BridgeToGatewayPlayerMsg.SkipToIndex(SkipToIndex(index = 1u))))

        val play = withTimeout(20.seconds) {
            var hit = fake.lastPlay
            while (hit == null) { delay(10); hit = fake.lastPlay }
            hit
        }
        assertEquals("spotify:playlist:1", play.first)
        assertEquals("spotify:track:3", play.second)
        h.companion.stop()
    }

    @Test
    fun `skipToIndex out of range no-ops`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song")))
        h.observer()!!.onQueue(SpQueue(previous = emptyList(), current = null, next = listOf(npTrack("spotify:track:2", "Next"))))
        h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.QueueChanged
        }

        h.driver.send(BridgeToGatewayMsgData.Player(BridgeToGatewayPlayerMsg.SkipToIndex(SkipToIndex(index = 9u))))

        delay(200)
        assertEquals(null, fake.lastPlay, "an out-of-range index must not issue a play")
        h.companion.stop()
    }

    // MARK: - auth lifecycle

    @Test
    fun `premium gate surfaces an auth failure`() = runBlocking {
        val fake = FakeClient()
        fake.productState = fake.productState.copy(isPremium = false, canUseSuperbird = false)
        fake.onConnect = { obs -> obs.onAuth(SpAuthState.LoggedIn(username = "u")) }
        val states = java.util.concurrent.CopyOnWriteArrayList<GlueAuthState>()
        val h = boot(this, fake) { states.add(it) }

        val failure = awaitState(states) { it is GlueAuthState.Failed } as GlueAuthState.Failed
        assertEquals("Spotify Premium is required", failure.reason)
        h.companion.stop()
    }

    @Test
    fun `device-flow pending surfaces the user code`() = runBlocking {
        val fake = FakeClient()
        fake.onConnect = { obs -> obs.onAuth(SpAuthState.Pending(url = "https://spotify.com/pair", code = "ABCD")) }
        val states = java.util.concurrent.CopyOnWriteArrayList<GlueAuthState>()
        val h = boot(this, fake) { states.add(it) }

        val pending = awaitState(states) { it is GlueAuthState.Pending && it.prompt != null } as GlueAuthState.Pending
        assertEquals("ABCD", pending.prompt?.userCode)
        assertEquals("https://spotify.com/pair", pending.prompt?.verificationUrl)
        h.companion.stop()
    }

    @Test
    fun `rust-provided saved drives liked without a contains round-trip`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onPlayer(state(npTrack("spotify:track:1", "Song", saved = true)))

        val snap = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        val ps = ((snap.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertEquals(true, ps.track?.liked)
        assertEquals(0, fake.favoritesContainsCalls, "liked must come from the rust-provided saved flag")
        h.companion.stop()
    }

    @Test
    fun `library change relays to the gateway`() = runBlocking {
        val fake = FakeClient()
        val h = boot(this, fake)
        h.observer()!!.onLibraryChanged(SpLibraryScope.PLAYLISTS)

        val ev = h.driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Library)?.data is GatewayToBridgeLibraryMsg.LibraryChanged
        }
        val changed = ((ev.data as GatewayToBridgeMsgData.Library).data as GatewayToBridgeLibraryMsg.LibraryChanged).data
        assertEquals(WireLibraryScope.Playlists, changed.scope)
        h.companion.stop()
    }

    private suspend fun awaitState(
        states: List<GlueAuthState>,
        predicate: (GlueAuthState) -> Boolean,
    ): GlueAuthState = withTimeout(20.seconds) {
        var hit = states.firstOrNull(predicate)
        while (hit == null) {
            delay(10)
            hit = states.firstOrNull(predicate)
        }
        hit
    }

    private companion object {
        fun item(uri: String, title: String, hasChildren: Boolean = false) = SpBrowseItem(
            uri = uri, title = title, subtitle = "", imageId = "ab67616d00001e02deadbeef",
            artists = emptyList(), album = SpAlbum(uri = "", name = "", imageId = ""),
            durationMs = 0u, saved = false, playable = true, hasChildren = hasChildren,
        )

        fun track(uri: String, name: String) = SpBrowseItem(
            uri = uri, title = name, subtitle = "Artist", imageId = "ab67616d00001e02deadbeef",
            artists = listOf(SpArtist(uri = "spotify:artist:1", name = "Artist")),
            album = SpAlbum(uri = "spotify:album:1", name = "Album", imageId = "ab67616d00001e02deadbeef"),
            durationMs = 1000u, saved = false, playable = true, hasChildren = false,
        )

        fun npTrack(uri: String, name: String, saved: Boolean = false) = SpTrack(
            uri = uri, uid = "", name = name,
            artists = listOf(SpArtist(uri = "spotify:artist:1", name = "Artist")),
            album = SpAlbum(uri = "spotify:album:1", name = "Album", imageId = "ab67616d00001e02deadbeef"),
            durationMs = 1000u, imageId = "ab67616d00001e02deadbeef", isEpisode = false, saved = saved, queued = false,
        )

        fun device(id: String, name: String, kind: SpDeviceKind, active: Boolean, volume: Float) =
            SpDevice(id = id, name = name, kind = kind, isActive = active, volume = volume)

        fun state(t: SpTrack, remote: Boolean = false) = SpPlayerState(
            track = t, contextUri = "spotify:playlist:1", contextName = "Ctx", isPaused = false,
            positionMs = 0u, durationMs = t.durationMs, shuffle = false, repeat = SpRepeat.OFF,
            playingRemotely = remote, remoteDeviceId = if (remote) "speaker" else "", onRemoteSpeaker = remote,
            canSeek = true, canSkipNext = true, canSkipPrev = true, canToggleShuffle = true,
            canRepeatContext = true, canRepeatTrack = true,
        )
    }
}
