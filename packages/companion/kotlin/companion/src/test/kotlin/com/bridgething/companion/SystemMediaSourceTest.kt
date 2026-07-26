package com.bridgething.companion

import com.bridgething.glue.NowPlayingSink
import com.bridgething.schema.AssetRequest
import com.bridgething.schema.BridgeToGatewayAssetMsg
import com.bridgething.schema.BridgeToGatewayLibraryMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayPlayerMsg
import com.bridgething.schema.FavoritesSet
import com.bridgething.schema.FavoritesToggle
import com.bridgething.schema.GatewayToBridgeAssetMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.GatewayToBridgePlayerMsg
import com.bridgething.schema.ItemKind
import com.bridgething.schema.ItemRef
import com.bridgething.schema.PlaybackState
import com.bridgething.schema.PlayerState
import com.bridgething.schema.QueueSnapshot
import com.bridgething.schema.RepeatMode
import com.bridgething.schema.ShuffleMode
import com.bridgething.schema.TransferBody
import io.mockk.mockk
import java.util.UUID
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import com.bridgething.schema.PlaybackTargets
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class SystemMediaSourceTest {
    private class RecordingSink : NowPlayingSink {
        data class Submission(val sourceId: String, val snapshot: PlayerState, val appBundle: String, val hasItem: Boolean)
        data class QueueSubmission(val sourceId: String, val queue: QueueSnapshot)
        data class TargetSubmission(val sourceId: String, val targets: PlaybackTargets)
        val players = CopyOnWriteArrayList<Submission>()
        val queues = CopyOnWriteArrayList<QueueSubmission>()
        val targets = CopyOnWriteArrayList<TargetSubmission>()
        val cleared = CopyOnWriteArrayList<String>()
        override fun submitPlayer(
            sourceId: String,
            snapshot: PlayerState,
            appBundle: String,
            hasItem: Boolean,
            wantsVolume: Boolean,
        ) {
            players.add(Submission(sourceId, snapshot, appBundle, hasItem))
        }
        override fun submitQueue(sourceId: String, queue: QueueSnapshot) { queues.add(QueueSubmission(sourceId, queue)) }
        override fun submitTargets(sourceId: String, targets: PlaybackTargets) {
            this.targets.add(TargetSubmission(sourceId, targets))
        }
        override fun clearSource(sourceId: String) { cleared.add(sourceId) }
    }

    private class FakeSession(
        override val packageName: String,
        @Volatile var snap: SystemMediaSnapshot?,
        @Volatile var artByToken: Map<String, SystemMediaArt> = emptyMap(),
    ) : SystemMediaSession {
        val calls = CopyOnWriteArrayList<String>()
        override fun snapshot(): SystemMediaSnapshot? = snap
        override suspend fun art(token: String): SystemMediaArt? = artByToken[token]
        override fun play() { calls.add("play") }
        override fun pause() { calls.add("pause") }
        override fun skipNext() { calls.add("skipNext") }
        override fun skipPrev() { calls.add("skipPrev") }
        override fun seekTo(positionMs: Long) { calls.add("seekTo:$positionMs") }
        override fun skipToQueueItem(queueId: Long) { calls.add("skipToQueueItem:$queueId") }
        override fun setShuffle(on: Boolean) { calls.add("setShuffle:$on") }
        override fun setRepeat(mode: RepeatMode) { calls.add("setRepeat:${mode.string}") }
        override fun setSpeed(speed: Float) { calls.add("setSpeed:$speed") }
        override fun setLiked(liked: Boolean) { calls.add("setLiked:$liked") }
    }

    private class FakeGateway : MediaSessionGateway {
        @Volatile override var isAccessGranted: Boolean = true
        @Volatile var sessions: List<SystemMediaSession> = emptyList()
        @Volatile private var onChanged: (() -> Unit)? = null
        override fun activeSessions(): List<SystemMediaSession> = sessions
        override fun listen(onChanged: () -> Unit): MediaSessionListenHandle {
            this.onChanged = onChanged
            return object : MediaSessionListenHandle {
                override fun stop() { this@FakeGateway.onChanged = null }
            }
        }
        fun emit(newSessions: List<SystemMediaSession>) {
            sessions = newSessions
            onChanged?.invoke()
        }
    }

    private fun playing(title: String, artist: String, pkg: String) =
        FakeSession(pkg, SystemMediaSnapshot(title, artist, "Album", 1000L, 250L, playing = true, canSeek = true))

    private fun paused(title: String, artist: String, pkg: String) =
        FakeSession(pkg, SystemMediaSnapshot(title, artist, "Album", 1000L, 250L, playing = false, canSeek = true))

    @Test
    fun `picks the playing foreign session and maps its metadata`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { setOf("com.spotify.client") }
        source.start()
        gw.emit(listOf(playing("Video", "Creator", "com.google.android.youtube")))

        val sub = sink.players.last()
        assertEquals(SystemMediaSource.SOURCE_ID, sub.sourceId)
        assertEquals("com.google.android.youtube", sub.appBundle)
        assertTrue(sub.hasItem)
        assertEquals("Video", sub.snapshot.track?.title)
        assertEquals("Creator", sub.snapshot.track?.artist)
        assertEquals(PlaybackState.Playing, sub.snapshot.playback.state)
        assertEquals(250u, sub.snapshot.playback.positionMs)
    }

    @Test
    fun `dedups a session whose android package the active glue owns`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { setOf("com.spotify.music") }
        source.start()
        gw.emit(listOf(playing("Song", "Artist", "com.spotify.music")))

        assertTrue(sink.players.isEmpty(), "spotify's own session must not be double-emitted")
        assertTrue(sink.cleared.isEmpty(), "nothing was ever audible, so no clear is sent")
    }

    @Test
    fun `clears on the transition from playing to none`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        gw.emit(listOf(playing("Video", "Creator", "com.google.android.youtube")))
        assertEquals(1, sink.players.size)
        gw.emit(listOf(paused("Video", "Creator", "com.google.android.youtube")))
        assertTrue(sink.cleared.contains(SystemMediaSource.SOURCE_ID))
    }

    @Test
    fun `does not resubmit an unchanged snapshot`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val yt = playing("Video", "Creator", "com.google.android.youtube")
        gw.emit(listOf(yt))
        gw.emit(listOf(yt))
        gw.emit(listOf(yt))
        assertEquals(1, sink.players.size, "an unchanged audible snapshot must not re-push over the link")
    }

    @Test
    fun `art token becomes an asset id and the asset resolves through the audible session`() = runBlocking {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val jpeg = SystemMediaArt(bytes = byteArrayOf(1, 2, 3), mime = "image/jpeg")
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot("Video", "Creator", "Album", 1000L, 0L, playing = true, canSeek = true, artToken = "bdeadbeef"),
            artByToken = mapOf("bdeadbeef" to jpeg),
        )
        gw.emit(listOf(yt))

        val artworkId = sink.players.last().snapshot.track?.artworkId
        assertEquals("${SystemMediaSource.ASSET_ID_PREFIX}bdeadbeef", artworkId)
        val served = source.asset(artworkId!!)
        assertEquals("image/jpeg", served?.mime)
        assertTrue(jpeg.bytes.contentEquals(served!!.bytes))
        assertEquals(null, source.asset("${SystemMediaSource.ASSET_ID_PREFIX}gone"), "an unresolvable token serves nothing")
        assertEquals(null, source.asset("img:not-ours"), "a non-system id is never served here")
    }

    @Test
    fun `queue maps to the upcoming window after the active item`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val queue = listOf(
            SystemMediaQueueEntry(queueId = 10L, title = "Played", subtitle = "A"),
            SystemMediaQueueEntry(queueId = 11L, title = "Current", subtitle = "B"),
            SystemMediaQueueEntry(queueId = 12L, title = "Next", subtitle = "C", artToken = "u123"),
            SystemMediaQueueEntry(queueId = 13L, title = "Later", subtitle = "D"),
        )
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot(
                "Current", "B", null, 1000L, 0L, playing = true, canSeek = true,
                queue = queue, activeQueueId = 11L,
            ),
        )
        gw.emit(listOf(yt))

        assertEquals(1, sink.queues.size)
        val snap = sink.queues.last().queue
        assertEquals(listOf("Next", "Later"), snap.items.map { it.title })
        assertEquals(snap.items.map { it.uri }, snap.order)
        assertEquals("${SystemMediaSource.ASSET_ID_PREFIX}u123", snap.items[0].artworkId)
        assertEquals("C", snap.items[0].artist)

        gw.emit(listOf(yt))
        assertEquals(1, sink.queues.size, "an unchanged queue must not re-push over the link")
    }

    @Test
    fun `skipToIndex routes to the queue id of the upcoming entry`() = runBlocking {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val queue = listOf(
            SystemMediaQueueEntry(queueId = 11L, title = "Current", subtitle = "B"),
            SystemMediaQueueEntry(queueId = 12L, title = "Next", subtitle = "C"),
            SystemMediaQueueEntry(queueId = 13L, title = "Later", subtitle = "D"),
        )
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot(
                "Current", "B", null, 1000L, 0L, playing = true, canSeek = true,
                queue = queue, activeQueueId = 11L,
            ),
        )
        gw.emit(listOf(yt))

        source.skipToIndex(1u)
        assertEquals(listOf("skipToQueueItem:13"), yt.calls.toList())
        source.skipToIndex(9u)
        assertEquals(1, yt.calls.size, "an out-of-range index is dropped")
    }

    @Test
    fun `full-fat session state maps onto the wire snapshot`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot(
                "Video", "Creator", "Album", 1000L, 250L, playing = true, canSeek = true,
                shuffle = true, repeat = RepeatMode.All, speed = 1.5f, positionAgeMs = 40L,
                liked = true, likeSupported = true, queueTitle = "My Mix",
            ),
        )
        gw.emit(listOf(yt))

        val ps = sink.players.last().snapshot
        assertTrue(ps.playback.shuffle)
        assertEquals(ShuffleMode.Songs, ps.playback.shuffleMode)
        assertEquals(RepeatMode.All, ps.playback.repeat)
        assertEquals(40u, ps.playback.positionAgeMs)
        assertEquals(1.5f, ps.options.speed)
        assertEquals(true, ps.track?.liked)
        assertEquals(true, ps.track?.isLikeSupported)
        assertEquals("My Mix", ps.context?.name)
    }

    @Test
    fun `unknown compat state degrades to the neutral card`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        gw.emit(listOf(playing("Video", "Creator", "com.google.android.youtube")))

        val ps = sink.players.last().snapshot
        assertEquals(false, ps.playback.shuffle)
        assertEquals(null, ps.playback.shuffleMode)
        assertEquals(RepeatMode.Off, ps.playback.repeat)
        assertEquals(1.0f, ps.options.speed)
        assertEquals(null, ps.track?.liked)
        assertEquals(null, ps.track?.isLikeSupported)
        assertEquals(null, ps.context)
    }

    @Test
    fun `positionAgeMs alone does not resubmit`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val base = SystemMediaSnapshot("Video", "Creator", null, 1000L, 250L, playing = true, canSeek = true, positionAgeMs = 10L)
        val yt = FakeSession("com.google.android.youtube", base)
        gw.emit(listOf(yt))
        yt.snap = base.copy(positionAgeMs = 900L)
        gw.emit(listOf(yt))
        assertEquals(1, sink.players.size, "a fresher read of the same position must not re-push")
    }

    @Test
    fun `state setters delegate to the audible session`() = runBlocking {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val yt = playing("Video", "Creator", "com.google.android.youtube")
        gw.emit(listOf(yt))

        source.setShuffle(true)
        source.setRepeat(RepeatMode.One)
        source.setSpeed(1.5f)
        source.setLiked(true)
        source.toggleLiked()
        assertEquals(
            listOf("setShuffle:true", "setRepeat:one", "setSpeed:1.5", "setLiked:true", "setLiked:true"),
            yt.calls.toList(),
        )
    }

    @Test
    fun `toggleLiked flips the last submitted liked state`() {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot("Video", "Creator", null, 1000L, 0L, playing = true, canSeek = true, liked = true, likeSupported = true),
        )
        gw.emit(listOf(yt))

        source.toggleLiked()
        assertEquals(listOf("setLiked:false"), yt.calls.toList())
    }

    @Test
    fun `transport verbs delegate to the audible session`() = runBlocking {
        val sink = RecordingSink()
        val gw = FakeGateway()
        val source = SystemMediaSource(gw, sink) { emptySet() }
        source.start()
        val yt = playing("Video", "Creator", "com.google.android.youtube")
        gw.emit(listOf(yt))

        source.pause()
        source.resume()
        source.skipNext()
        source.skipPrev()
        source.seekTo(5000u)
        assertEquals(listOf("pause", "play", "skipNext", "skipPrev", "seekTo:5000"), yt.calls.toList())
    }

    private suspend fun boot(scope: CoroutineScope, gw: FakeGateway, withGlue: Boolean = true): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "sysmedia-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
            mediaSessions = gw,
        )
        if (withGlue) companion.attach(FakeGlue())
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    @Test
    fun `system session snapshot reaches the wire and an inbound pause routes to it`() = runBlocking {
        val gw = FakeGateway()
        val (companion, driver) = boot(this, gw)
        val yt = playing("Video", "Creator", "com.google.android.youtube")
        gw.emit(listOf(yt))

        val snap = driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        val ps = ((snap.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data
        assertEquals("Video", ps.track?.title)

        driver.send(BridgeToGatewayMsgData.Player(BridgeToGatewayPlayerMsg.Pause))
        withTimeout(20.seconds) { while (yt.calls.isEmpty()) delay(10) }
        assertEquals("pause", yt.calls.first())
        companion.stop()
    }

    @Test
    fun `system art asset id round-trips over the wire`() = runBlocking {
        val gw = FakeGateway()
        val (companion, driver) = boot(this, gw)
        val jpeg = SystemMediaArt(bytes = byteArrayOf(9, 8, 7, 6), mime = "image/jpeg")
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot("Video", "Creator", null, 1000L, 0L, playing = true, canSeek = true, artToken = "bcafe"),
            artByToken = mapOf("bcafe" to jpeg),
        )
        gw.emit(listOf(yt))

        val snap = driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        val artworkId = ((snap.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data.track?.artworkId
        assertEquals("${SystemMediaSource.ASSET_ID_PREFIX}bcafe", artworkId)

        val resp = driver.request(
            BridgeToGatewayMsgData.Asset(BridgeToGatewayAssetMsg.Request(AssetRequest(id = artworkId!!, requestId = UUID.randomUUID()))),
        )
        val got = (resp.data as GatewayToBridgeMsgData.Asset).data as GatewayToBridgeAssetMsg.Got
        assertEquals("image/jpeg", got.data.mime)
        assertTrue(jpeg.bytes.contentEquals((got.data.body as TransferBody.Inline).data))
        companion.stop()
    }

    @Test
    fun `session queue reaches the wire as queueChanged`() = runBlocking {
        val gw = FakeGateway()
        val (companion, driver) = boot(this, gw)
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot(
                "Current", "B", null, 1000L, 0L, playing = true, canSeek = true,
                queue = listOf(
                    SystemMediaQueueEntry(queueId = 1L, title = "Current", subtitle = "B"),
                    SystemMediaQueueEntry(queueId = 2L, title = "Next", subtitle = "C"),
                ),
                activeQueueId = 1L,
            ),
        )
        gw.emit(listOf(yt))

        val queueMsg = driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.QueueChanged
        }
        val queue = ((queueMsg.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.QueueChanged).data
        assertEquals(listOf("Next"), queue.items.map { it.title })
        companion.stop()
    }

    @Test
    fun `a like for the system uri routes to the session rating, not the glue`() = runBlocking {
        val gw = FakeGateway()
        val (companion, driver) = boot(this, gw)
        val yt = FakeSession(
            "com.google.android.youtube",
            SystemMediaSnapshot("Video", "Creator", null, 1000L, 0L, playing = true, canSeek = true, liked = false, likeSupported = true),
        )
        gw.emit(listOf(yt))

        val snap = driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        val uri = ((snap.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data.track!!.uri!!

        driver.send(
            BridgeToGatewayMsgData.Library(
                BridgeToGatewayLibraryMsg.FavoritesSet(FavoritesSet(item = ItemRef(uri = uri, kind = ItemKind.Track), liked = true)),
            ),
        )
        withTimeout(20.seconds) { while (yt.calls.isEmpty()) delay(10) }
        assertEquals("setLiked:true", yt.calls.first())

        yt.snap = yt.snap!!.copy(liked = true)
        gw.emit(listOf(yt))
        driver.send(
            BridgeToGatewayMsgData.Library(
                BridgeToGatewayLibraryMsg.FavoritesToggle(FavoritesToggle(item = ItemRef(uri = uri, kind = ItemKind.Track))),
            ),
        )
        withTimeout(20.seconds) { while (yt.calls.size < 2) delay(10) }
        assertEquals("setLiked:false", yt.calls[1], "toggle flips the submitted liked state")
        companion.stop()
    }

    @Test
    fun `controls a foreign session with no provider signed in`() = runBlocking {
        val gw = FakeGateway()
        val (companion, driver) = boot(this, gw, withGlue = false)
        val yt = playing("Video", "Creator", "com.google.android.youtube")
        gw.emit(listOf(yt))

        val snap = driver.waitOutbound(20.seconds) {
            (it.data as? GatewayToBridgeMsgData.Player)?.data is GatewayToBridgePlayerMsg.Snapshot
        }
        assertEquals("Video", ((snap.data as GatewayToBridgeMsgData.Player).data as GatewayToBridgePlayerMsg.Snapshot).data.track?.title)

        driver.send(BridgeToGatewayMsgData.Player(BridgeToGatewayPlayerMsg.Pause))
        withTimeout(20.seconds) { while (yt.calls.isEmpty()) delay(10) }
        assertEquals("pause", yt.calls.first())
        companion.stop()
    }
}
