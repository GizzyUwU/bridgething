package com.bridgething.companion

import android.content.Context
import android.media.AudioManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.bridgething.companion.shell.AndroidAudioBackend
import java.util.UUID
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.abs
import kotlin.math.roundToInt
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.bridgething_companion.EarconSink
import uniffi.bridgething_companion.NoHandle
import uniffi.bridgething_companion.SpeakSink

private class RecordingSpeakSink : SpeakSink(NoHandle) {
    val started = AtomicBoolean(false)
    val finished = LinkedBlockingQueue<Boolean>()

    override fun onStart() {
        started.set(true)
    }

    override fun onFinished(ok: Boolean) {
        finished.add(ok)
    }
}

private class RecordingEarconSink : EarconSink(NoHandle) {
    val finished = LinkedBlockingQueue<Boolean>()

    override fun onFinished(ok: Boolean) {
        finished.add(ok)
    }
}

@RunWith(AndroidJUnit4::class)
class AndroidAudioBackendTest {
    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun realTtsSpeaksAndCompletes() {
        val backend = AndroidAudioBackend(context)
        val sink = RecordingSpeakSink()
        backend.speak(UUID.randomUUID().toString(), "bridgething audio check", null, sink)
        val completed = sink.finished.poll(30, TimeUnit.SECONDS)
        assertNotNull("speech should finish within the deadline", completed)
        assertTrue("onStart should fire when speech begins", sink.started.get())
        assertTrue("real TextToSpeech should run the utterance to completion", completed!!)
    }

    @Test
    fun realSetVolumeMovesStreamVolume() {
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
    fun earconUnknownNameFinishesFalse() {
        val backend = AndroidAudioBackend(context)
        val sink = RecordingEarconSink()
        backend.playEarcon("does-not-exist", sink)
        assertEquals("no earcon assets are bundled, so unknown names finish false", false, sink.finished.poll(10, TimeUnit.SECONDS))
    }
}
