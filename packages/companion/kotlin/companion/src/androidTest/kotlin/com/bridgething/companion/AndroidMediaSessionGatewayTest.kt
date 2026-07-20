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
import com.bridgething.schema.RepeatMode
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class AndroidMediaSessionGatewayTest {
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
        session = MediaSession(context, "bridgething-gateway-test")
        val gateway = AndroidMediaSessionGateway(context, listenerComponent)
        waitUntil("notification listener grant") { gateway.isAccessGranted }
    }

    @After
    fun teardown() {
        runCatching { session.release() }
        runCatching { thread.quitSafely() }
        shell("cmd notification disallow_listener ${listenerComponent.flattenToString()}")
    }

    @Test
    fun readsPublishedSessionWithArtAndQueue() = runBlocking {
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

        val gateway = AndroidMediaSessionGateway(context, listenerComponent)
        val mine = waitForNotNull("published session visible") {
            gateway.activeSessions().firstOrNull { it.packageName == context.packageName }
        }
        val snap = waitForNotNull("session snapshot") { mine.snapshot() }

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
        val served = mine.art(snap.artToken!!)
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

        val gateway = AndroidMediaSessionGateway(context, listenerComponent)
        val mine = waitForNotNull("published session visible") {
            gateway.activeSessions().firstOrNull { it.packageName == context.packageName }
        }
        mine.pause()
        assertTrue("onPause received", paused.await(10, TimeUnit.SECONDS))
        mine.skipToQueueItem(2L)
        assertTrue("onSkipToQueueItem received", skippedTo.await(10, TimeUnit.SECONDS))
    }

    @Test
    fun listenFiresOnMetadataChange() {
        session.setPlaybackState(playing(activeQueueId = 1L))
        session.setMetadata(
            MediaMetadata.Builder().putText(MediaMetadata.METADATA_KEY_TITLE, "First").build(),
        )
        session.setActive(true)

        val gateway = AndroidMediaSessionGateway(context, listenerComponent)
        val changed = CountDownLatch(1)
        val handle = gateway.listen { changed.countDown() }
        try {
            session.setMetadata(
                MediaMetadata.Builder().putText(MediaMetadata.METADATA_KEY_TITLE, "Second").build(),
            )
            assertTrue("listener notified of metadata change", changed.await(10, TimeUnit.SECONDS))
        } finally {
            handle.stop()
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

            val gateway = AndroidMediaSessionGateway(context, listenerComponent)
            val snap = waitForNotNull("compat state readable") {
                gateway.activeSessions().firstOrNull { it.packageName == context.packageName }
                    ?.snapshot()?.takeIf { it.shuffle == true }
            }
            assertEquals(RepeatMode.One, snap.repeat)
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

            val gateway = AndroidMediaSessionGateway(context, listenerComponent)
            val mine = waitForNotNull("compat session visible") {
                gateway.activeSessions().firstOrNull { it.packageName == context.packageName }
            }
            mine.setShuffle(true)
            assertTrue("onSetShuffleMode received", shuffled.await(10, TimeUnit.SECONDS))
            mine.setRepeat(RepeatMode.All)
            assertTrue("onSetRepeatMode received", repeated.await(10, TimeUnit.SECONDS))
            mine.setLiked(true)
            assertTrue("onSetRating received", rated.await(10, TimeUnit.SECONDS))
        } finally {
            compatSession.release()
        }
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
