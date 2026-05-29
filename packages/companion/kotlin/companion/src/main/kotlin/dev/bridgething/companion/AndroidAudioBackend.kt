package dev.bridgething.companion

import android.content.Context
import android.media.AudioManager
import android.media.MediaPlayer
import android.os.Bundle
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import java.util.Locale
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import kotlin.coroutines.resume
import kotlin.math.roundToInt
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.suspendCancellableCoroutine

// TextToSpeech has no per-utterance cancel; cancel/cancelAll both stop the current utterance
public class AndroidAudioBackend(
    context: Context,
) : AudioBackend {
    private val appContext = context.applicationContext
    private val audio = appContext.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val ready = CompletableDeferred<Boolean>()
    private val callbacks = ConcurrentHashMap<String, Lifecycle>()

    private val tts = TextToSpeech(appContext) { status ->
        ready.complete(status == TextToSpeech.SUCCESS)
    }.apply {
        setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String?) {
                utteranceId?.let { callbacks[it]?.onStart?.invoke() }
            }

            override fun onDone(utteranceId: String?) = finish(utteranceId, completed = true)

            @Deprecated("required abstract override; the int-code variant supersedes on newer API levels")
            override fun onError(utteranceId: String?) = finish(utteranceId, completed = false)

            override fun onError(utteranceId: String?, errorCode: Int) = finish(utteranceId, completed = false)

            override fun onStop(utteranceId: String?, interrupted: Boolean) = finish(utteranceId, completed = false)
        })
    }

    private class Lifecycle(val onStart: () -> Unit, val onFinish: (Boolean) -> Unit)

    private fun finish(utteranceId: String?, completed: Boolean) {
        val id = utteranceId ?: return
        callbacks.remove(id)?.onFinish?.invoke(completed)
    }

    public override suspend fun setVolume(level: Float) {
        val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
        val index = (level.coerceIn(0f, 1f) * max).roundToInt()
        runCatching { audio.setStreamVolume(AudioManager.STREAM_MUSIC, index, 0) }
    }

    public override suspend fun setMute(muted: Boolean) {
        val direction = if (muted) AudioManager.ADJUST_MUTE else AudioManager.ADJUST_UNMUTE
        runCatching { audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, direction, 0) }
    }

    public override suspend fun volumeUp() {
        runCatching { audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, AudioManager.ADJUST_RAISE, 0) }
    }

    public override suspend fun volumeDown() {
        runCatching { audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, AudioManager.ADJUST_LOWER, 0) }
    }

    public override suspend fun muteToggle() {
        runCatching { audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, AudioManager.ADJUST_TOGGLE_MUTE, 0) }
    }

    public override suspend fun speak(id: UUID, text: String, voice: String?, onStart: () -> Unit): Boolean {
        if (!ready.await()) return false
        applyVoice(voice)
        val uid = id.toString()
        return suspendCancellableCoroutine { cont ->
            callbacks[uid] = Lifecycle(onStart) { completed -> if (cont.isActive) cont.resume(completed) }
            cont.invokeOnCancellation { callbacks.remove(uid) }
            val result = tts.speak(text, TextToSpeech.QUEUE_FLUSH, Bundle(), uid)
            if (result != TextToSpeech.SUCCESS) {
                callbacks.remove(uid)
                if (cont.isActive) cont.resume(false)
            }
        }
    }

    private fun applyVoice(voice: String?) {
        if (voice == null) return
        val match = tts.voices?.firstOrNull { it.name == voice }
        if (match != null) {
            tts.voice = match
            return
        }
        runCatching { tts.language = Locale.forLanguageTag(voice) }
    }

    public override suspend fun cancel(id: UUID) {
        runCatching { tts.stop() }
    }

    public override suspend fun cancelAll() {
        runCatching { tts.stop() }
    }

    public override suspend fun playEarcon(name: String): Boolean {
        val resId = appContext.resources.getIdentifier(name, "raw", appContext.packageName)
        if (resId == 0) return false
        val player = MediaPlayer.create(appContext, resId) ?: return false
        player.setOnCompletionListener { it.release() }
        player.start()
        return true
    }
}
