package com.bridgething.companion

import android.content.ComponentName
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.media.MediaDescription
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Handler
import android.os.HandlerThread
import android.os.ParcelFileDescriptor
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.RatingCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.bridgething.companion.shell.AndroidMediaSessionBackend
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.bridgething_companion.MediaArt
import uniffi.bridgething_companion.MediaArtSink
import uniffi.bridgething_companion.MediaControl
import uniffi.bridgething_companion.MediaRepeatMode
import uniffi.bridgething_companion.MediaSessionInbox
import uniffi.bridgething_companion.MediaSessionSnapshot
import uniffi.bridgething_companion.MediaSnapshotSink
import uniffi.bridgething_companion.NoHandle

private class RecordingInbox : MediaSessionInbox(NoHandle) {
    val changes = LinkedBlockingQueue<Unit>()

    override fun onSessionsChanged() {
        changes.add(Unit)
    }
}

private class RecordingSnapshotSink : MediaSnapshotSink(NoHandle) {
    val results = LinkedBlockingQueue<List<MediaSessionSnapshot>>()

    override fun complete(sessions: List<MediaSessionSnapshot>) {
        results.add(sessions)
    }
}

private class RecordingArtSink : MediaArtSink(NoHandle) {
    val results = LinkedBlockingQueue<Optional>()

    class Optional(val art: MediaArt?)

    override fun complete(art: MediaArt?) {
        results.add(Optional(art))
    }
}

@RunWith(AndroidJUnit4::class)
class AndroidMediaSessionBackendTest {
    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    private val listenerComponent: ComponentName
        get() = ComponentName(context, TestNotificationListener::class.java)

    private lateinit var thread: HandlerThread
    private lateinit var session: MediaSession

    @Before
    fun grantAndPublish() {
        shell("cmd notification allow_listener ${listenerComponent.flattenToString()}")
        thread = HandlerThread("test-media-session").apply { start() }
        session = MediaSession(context, "bridgething-backend-test")
        val backend = AndroidMediaSessionBackend(context, listenerComponent)
        waitUntil("notification listener grant") { backend.isAccessGranted() }
    }

    @After
    fun teardown() {
        runCatching { session.release() }
        runCatching { thread.quitSafely() }
        shell("cmd notification disallow_listener ${listenerComponent.flattenToString()}")
    }

    @Test
    fun readsPublishedSessionWithArtAndQueue() {
        val art = Bitmap.createBitmap(64, 64, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.MAGENTA) }
        session.setMetadata(
            MediaMetadata.Builder()
                .putText(MediaMetadata.METADATA_KEY_TITLE, "Test Track")
                .putText(MediaMetadata.METADATA_KEY_ARTIST, "Test Artist")
                .putText(MediaMetadata.METADATA_KEY_ALBUM, "Test Album")
                .putLong(MediaMetadata.METADATA_KEY_DURATION, 90_000L)
                .putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, art)
                .build(),
        )
        session.setQueue(
            listOf(
                MediaSession.QueueItem(description("Test Track", "Test Artist"), 1L),
                MediaSession.QueueItem(description("Next Track", "Test Artist"), 2L),
            ),
        )
        session.setPlaybackState(playing(activeQueueId = 1L))
        session.setActive(true)

        val backend = AndroidMediaSessionBackend(context, listenerComponent)
        val snap = waitForNotNull("published session snapshot") { snapshotOf(backend) }

        assertEquals("Test Track", snap.title)
        assertEquals("Test Artist", snap.artist)
        assertEquals("Test Album", snap.album)
        assertEquals(90_000L, snap.durationMs)
        assertTrue(snap.playing)
        assertTrue(snap.canSeek)
        assertEquals(listOf(1L, 2L), snap.queue.map { it.queueId })
        assertEquals(listOf("Test Track", "Next Track"), snap.queue.map { it.title })
        assertEquals(1L, snap.activeQueueId)

        assertNotNull("art token derived from the published bitmap", snap.artToken)
        val artSink = RecordingArtSink()
        backend.art(context.packageName, snap.artToken!!, artSink)
        val served = artSink.results.poll(10, TimeUnit.SECONDS)?.art
        assertNotNull("art bytes served", served)
        assertEquals("image/jpeg", served!!.mime)
        val decoded = BitmapFactory.decodeByteArray(served.bytes, 0, served.bytes.size)
        assertNotNull("served art decodes as an image", decoded)
        assertEquals(64, decoded.width)
    }

    @Test
    fun transportControlsReachTheSessionCallback() {
        val paused = CountDownLatch(1)
        val skippedTo = CountDownLatch(1)
        session.setCallback(
            object : MediaSession.Callback() {
                override fun onPause() = paused.countDown()

                override fun onSkipToQueueItem(id: Long) {
                    if (id == 2L) skippedTo.countDown()
                }
            },
            Handler(thread.looper),
        )
        session.setPlaybackState(playing(activeQueueId = 1L))
        session.setMetadata(
            MediaMetadata.Builder().putText(MediaMetadata.METADATA_KEY_TITLE, "Test Track").build(),
        )
        session.setActive(true)

        val backend = AndroidMediaSessionBackend(context, listenerComponent)
        waitForNotNull("published session snapshot") { snapshotOf(backend) }
        backend.control(context.packageName, MediaControl.Pause)
        assertTrue("onPause received", paused.await(10, TimeUnit.SECONDS))
        backend.control(context.packageName, MediaControl.SkipToQueueItem(2L))
        assertTrue("onSkipToQueueItem received", skippedTo.await(10, TimeUnit.SECONDS))
    }

    @Test
    fun startFiresInboxOnMetadataChange() {
        session.setPlaybackState(playing(activeQueueId = 1L))
        session.setMetadata(
            MediaMetadata.Builder().putText(MediaMetadata.METADATA_KEY_TITLE, "First").build(),
        )
        session.setActive(true)

        val backend = AndroidMediaSessionBackend(context, listenerComponent)
        waitForNotNull("published session snapshot") { snapshotOf(backend) }
        val inbox = RecordingInbox()
        backend.start(inbox)
        try {
            inbox.changes.clear()
            session.setMetadata(
                MediaMetadata.Builder().putText(MediaMetadata.METADATA_KEY_TITLE, "Second").build(),
            )
            assertTrue("inbox notified of metadata change", inbox.changes.poll(10, TimeUnit.SECONDS) != null)
        } finally {
            backend.stop()
        }
    }

    @Test
    fun compatSessionExposesShuffleRepeatSpeedAndRating() {
        val compatSession = MediaSessionCompat(context, "bridgething-compat-test")
        try {
            compatSession.setRatingType(RatingCompat.RATING_HEART)
            compatSession.setMetadata(
                MediaMetadataCompat.Builder()
                    .putText(MediaMetadataCompat.METADATA_KEY_TITLE, "Compat Track")
                    .putRating(MediaMetadataCompat.METADATA_KEY_USER_RATING, RatingCompat.newHeartRating(true))
                    .build(),
            )
            compatSession.setPlaybackState(
                PlaybackStateCompat.Builder()
                    .setState(PlaybackStateCompat.STATE_PLAYING, 1000L, 1.5f)
                    .setActions(
                        PlaybackStateCompat.ACTION_PLAY or PlaybackStateCompat.ACTION_PAUSE or
                            PlaybackStateCompat.ACTION_SET_SHUFFLE_MODE or PlaybackStateCompat.ACTION_SET_REPEAT_MODE or
                            PlaybackStateCompat.ACTION_SET_RATING,
                    )
                    .build(),
            )
            compatSession.setShuffleMode(PlaybackStateCompat.SHUFFLE_MODE_ALL)
            compatSession.setRepeatMode(PlaybackStateCompat.REPEAT_MODE_ONE)
            compatSession.setQueueTitle("Compat Mix")
            compatSession.isActive = true

            val backend = AndroidMediaSessionBackend(context, listenerComponent)
            val snap = waitForNotNull("compat state readable") {
                snapshotOf(backend)?.takeIf { it.shuffle == true }
            }
            assertEquals(MediaRepeatMode.ONE, snap.repeat)
            assertEquals(1.5f, snap.speed)
            assertNotNull("position age stamped while playing", snap.positionAgeMs)
            assertEquals(true, snap.liked)
            assertTrue("heart rating settable", snap.likeSupported)
            assertEquals("Compat Mix", snap.queueTitle)
        } finally {
            compatSession.release()
        }
    }

    @Test
    fun compatSettersReachTheSessionCallback() {
        val shuffled = CountDownLatch(1)
        val repeated = CountDownLatch(1)
        val rated = CountDownLatch(1)
        val compatSession = MediaSessionCompat(context, "bridgething-compat-set-test")
        try {
            compatSession.setCallback(
                object : MediaSessionCompat.Callback() {
                    override fun onSetShuffleMode(shuffleMode: Int) {
                        if (shuffleMode == PlaybackStateCompat.SHUFFLE_MODE_ALL) shuffled.countDown()
                    }

                    override fun onSetRepeatMode(repeatMode: Int) {
                        if (repeatMode == PlaybackStateCompat.REPEAT_MODE_ALL) repeated.countDown()
                    }

                    override fun onSetRating(rating: RatingCompat?) {
                        if (rating?.hasHeart() == true) rated.countDown()
                    }
                },
                Handler(thread.looper),
            )
            compatSession.setRatingType(RatingCompat.RATING_HEART)
            compatSession.setMetadata(
                MediaMetadataCompat.Builder().putText(MediaMetadataCompat.METADATA_KEY_TITLE, "Compat Track").build(),
            )
            compatSession.setPlaybackState(
                PlaybackStateCompat.Builder().setState(PlaybackStateCompat.STATE_PLAYING, 0L, 1.0f).setActions(
                    PlaybackStateCompat.ACTION_SET_SHUFFLE_MODE or PlaybackStateCompat.ACTION_SET_REPEAT_MODE or
                        PlaybackStateCompat.ACTION_SET_RATING,
                ).build(),
            )
            compatSession.isActive = true

            val backend = AndroidMediaSessionBackend(context, listenerComponent)
            waitForNotNull("compat session snapshot") { snapshotOf(backend) }
            backend.control(context.packageName, MediaControl.SetShuffle(true))
            assertTrue("onSetShuffleMode received", shuffled.await(10, TimeUnit.SECONDS))
            backend.control(context.packageName, MediaControl.SetRepeat(MediaRepeatMode.ALL))
            assertTrue("onSetRepeatMode received", repeated.await(10, TimeUnit.SECONDS))
            backend.control(context.packageName, MediaControl.SetLiked(true))
            assertTrue("onSetRating received", rated.await(10, TimeUnit.SECONDS))
        } finally {
            compatSession.release()
        }
    }

    private fun snapshotOf(backend: AndroidMediaSessionBackend): MediaSessionSnapshot? {
        val sink = RecordingSnapshotSink()
        backend.snapshotAll(sink)
        val sessions = sink.results.poll(10, TimeUnit.SECONDS) ?: return null
        return sessions.firstOrNull { it.`package` == context.packageName }
    }

    private fun description(title: String, subtitle: String): MediaDescription =
        MediaDescription.Builder().setTitle(title).setSubtitle(subtitle).build()

    private fun playing(activeQueueId: Long): PlaybackState =
        PlaybackState.Builder()
            .setState(PlaybackState.STATE_PLAYING, 1000L, 1.0f)
            .setActions(
                PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or
                    PlaybackState.ACTION_SKIP_TO_NEXT or PlaybackState.ACTION_SKIP_TO_PREVIOUS or
                    PlaybackState.ACTION_SEEK_TO or PlaybackState.ACTION_SKIP_TO_QUEUE_ITEM,
            )
            .setActiveQueueItemId(activeQueueId)
            .build()

    private fun shell(command: String) {
        val fd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
        ParcelFileDescriptor.AutoCloseInputStream(fd).use { it.readBytes() }
    }

    private fun waitUntil(what: String, deadlineMs: Long = 10_000, condition: () -> Boolean) {
        val end = System.currentTimeMillis() + deadlineMs
        while (System.currentTimeMillis() < end) {
            if (condition()) return
            Thread.sleep(100)
        }
        throw AssertionError("timed out waiting for $what")
    }

    private fun <T : Any> waitForNotNull(what: String, deadlineMs: Long = 10_000, supplier: () -> T?): T {
        val end = System.currentTimeMillis() + deadlineMs
        while (System.currentTimeMillis() < end) {
            supplier()?.let { return it }
            Thread.sleep(100)
        }
        throw AssertionError("timed out waiting for $what")
    }
}
