package dev.bridgething.companion

import dev.bridgething.glue.NowPlayingSink
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.BridgeToGatewayPlayerMsg
import dev.bridgething.schema.GatewayToBridgeMsgData
import dev.bridgething.schema.GatewayToBridgePlayerMsg
import dev.bridgething.schema.PlaybackState
import dev.bridgething.schema.PlayerState
import dev.bridgething.schema.QueueSnapshot
import io.mockk.mockk
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * drives [SystemMediaSource] over a fake [MediaSessionGateway] (no real MediaSessionManager), asserting the
 * arbitration (playing-only), provider-package dedup, and metadata mapping, then drives the whole companion
 * so a system session's snapshot reaches the wire and an inbound Pause routes to that session's controls.
 */
class SystemMediaSourceTest {
    private class RecordingSink : NowPlayingSink {
        data class Submission(val sourceId: String, val snapshot: PlayerState, val appBundle: String, val hasItem: Boolean)
        val players = CopyOnWriteArrayList<Submission>()
        val cleared = CopyOnWriteArrayList<String>()
        override fun submitPlayer(sourceId: String, snapshot: PlayerState, appBundle: String, hasItem: Boolean) {
            players.add(Submission(sourceId, snapshot, appBundle, hasItem))
        }
        override fun submitQueue(sourceId: String, queue: QueueSnapshot) {}
        override fun clearSource(sourceId: String) { cleared.add(sourceId) }
    }

    private class FakeSession(
        override val packageName: String,
        @Volatile var snap: SystemMediaSnapshot?,
    ) : SystemMediaSession {
        val calls = CopyOnWriteArrayList<String>()
        override fun snapshot(): SystemMediaSnapshot? = snap
        override fun play() { calls.add("play") }
        override fun pause() { calls.add("pause") }
        override fun skipNext() { calls.add("skipNext") }
        override fun skipPrev() { calls.add("skipPrev") }
        override fun seekTo(positionMs: Long) { calls.add("seekTo:$positionMs") }
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
        if (withGlue) companion.setActive(FakeGlue())
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
