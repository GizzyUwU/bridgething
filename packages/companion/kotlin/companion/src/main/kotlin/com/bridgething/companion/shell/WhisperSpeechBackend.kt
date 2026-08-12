package com.bridgething.companion.shell

import com.bridgething.asr.whisper.WhisperRecognizer
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.bridgething_companion.PrepareSink
import uniffi.bridgething_companion.SpeechRecognizer
import uniffi.bridgething_companion.SpeechSegment
import uniffi.bridgething_companion.Transcription
import uniffi.bridgething_companion.TranscriptionSink

public class WhisperSpeechBackend(
    private val modelPath: () -> String?,
) : SpeechRecognizer {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default + CoroutineName("bridgething-whisper"))
    private val lock = Mutex()
    private var loaded: Pair<String, WhisperRecognizer>? = null

    override fun prepare(sink: PrepareSink) {
        scope.launch {
            sink.use {
                try {
                    engine() ?: run {
                        it.onFailed("no asr model installed")
                        return@use
                    }
                    it.onReady()
                } catch (t: Throwable) {
                    it.onFailed(t.message ?: t.toString())
                }
            }
        }
    }

    override fun transcribe(pcm: List<Float>, sampleRateHz: UInt, sink: TranscriptionSink) {
        scope.launch {
            sink.use { held ->
                try {
                    val engine = engine() ?: run {
                        held.fail("no asr model installed")
                        return@use
                    }
                    val result = engine.transcribeDetailed(pcm.toFloatArray(), sampleRateHz.toInt())
                    held.complete(
                        Transcription(
                            text = result.text,
                            alternatives = emptyList(),
                            segments = result.segments.map {
                                SpeechSegment(
                                    text = it.text,
                                    startMs = it.startMs.coerceAtLeast(0L).toULong(),
                                    endMs = it.endMs.coerceAtLeast(0L).toULong(),
                                    confidence = it.confidence,
                                )
                            },
                            confidence = result.confidence,
                        ),
                    )
                } catch (t: Throwable) {
                    held.fail(t.message ?: t.toString())
                }
            }
        }
    }

    private suspend fun engine(): WhisperRecognizer? = lock.withLock {
        val path = modelPath() ?: return null
        loaded?.let { (heldPath, engine) ->
            if (heldPath == path) return engine
            runCatching { engine.close() }
        }
        val engine = WhisperRecognizer(modelPath = path)
        engine.prepare()
        loaded = path to engine
        engine
    }
}
