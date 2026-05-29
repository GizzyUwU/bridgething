package dev.bridgething.companion

import android.content.Context
import android.media.AudioManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlin.time.Duration.Companion.seconds

/**
 * Real-backend tier: drives the actual [AndroidAudioBackend] against the
 * device / emulator framework (TextToSpeech, AudioManager) with no mocking. The
 * JVM unit suite ([AudioDispatchTest]) covers dispatch routing with a fake; this
 * proves the platform really speaks and really moves stream volume.
 */
@RunWith(AndroidJUnit4::class)
class AndroidAudioBackendTest {
    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun realTtsSpeaksAndCompletes() = runBlocking {
        val backend = AndroidAudioBackend(context)
        val started = AtomicBoolean(false)
        val completed = withTimeout(30.seconds) {
            backend.speak(UUID.randomUUID(), "bridgething audio check", null) { started.set(true) }
        }
        assertTrue("onStart should fire when speech begins", started.get())
        assertTrue("real TextToSpeech should run the utterance to completion", completed)
    }

    @Test
    fun realSetVolumeMovesStreamVolume() = runBlocking {
        val backend = AndroidAudioBackend(context)
        val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
        val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)

        backend.setVolume(0.5f)
        val expected = (0.5f * max).roundToInt()
        val actual = audio.getStreamVolume(AudioManager.STREAM_MUSIC)
        assertTrue(
            "stream volume should land near the requested level (expected ~$expected, got $actual of $max)",
            abs(actual - expected) <= 1,
        )
    }

    @Test
    fun earconUnknownNameReturnsFalse() = runBlocking {
        val backend = AndroidAudioBackend(context)
        assertFalse("no earcon assets are bundled yet, so unknown names are not-found", backend.playEarcon("does-not-exist"))
    }
}
