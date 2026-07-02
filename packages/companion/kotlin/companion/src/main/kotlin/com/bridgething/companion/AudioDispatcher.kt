package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.audio
import com.bridgething.glue.BridgethingGlue
import com.bridgething.schema.Tts
import com.bridgething.schema.TtsEnded
import com.bridgething.schema.TtsStarted
import com.bridgething.schema.VolumeChanged
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

public class AudioDispatcher(
    private val backend: AudioBackend,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val mutex = Mutex()
    private val jobs = mutableListOf<Job>()

    @Volatile
    private var glueProvider: (suspend () -> BridgethingGlue?)? = null

    public fun setGlueProvider(provider: suspend () -> BridgethingGlue?) {
        glueProvider = provider
    }

    private suspend fun volumeGlue(): BridgethingGlue? {
        val glue = glueProvider?.invoke() ?: return null
        return if (glue.ownsVolume()) glue else null
    }

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            stopJobs()
            jobs.add(
                scope.launch {
                    gateway.audio.volumeUp.collect {
                        val glue = volumeGlue()
                        if (glue != null) {
                            runCatching { glue.volumeUp() }
                                .onSuccess { level -> broadcastVolume(gateway, level) }
                        } else {
                            backend.volumeUp()
                        }
                    }
                },
            )
            jobs.add(
                scope.launch {
                    gateway.audio.volumeDown.collect {
                        val glue = volumeGlue()
                        if (glue != null) {
                            runCatching { glue.volumeDown() }
                                .onSuccess { level -> broadcastVolume(gateway, level) }
                        } else {
                            backend.volumeDown()
                        }
                    }
                },
            )
            jobs.add(
                scope.launch {
                    gateway.audio.setVolume.collect { (_, msg) ->
                        val glue = volumeGlue()
                        if (glue != null) {
                            runCatching { glue.setVolume(msg.level) }
                                .onSuccess { level -> broadcastVolume(gateway, level) }
                        } else {
                            backend.setVolume(msg.level)
                        }
                    }
                },
            )
            jobs.add(
                scope.launch {
                    gateway.audio.muteToggle.collect {
                        // connect has no mute surface; swallow rather than mute the phone
                        if (volumeGlue() == null) backend.muteToggle()
                    }
                },
            )
            jobs.add(
                scope.launch {
                    gateway.audio.setMute.collect { (_, msg) ->
                        if (volumeGlue() == null) backend.setMute(msg.muted)
                    }
                },
            )
            jobs.add(scope.launch { gateway.audio.ttsCancel.collect { (_, msg) -> backend.cancel(msg.id) } })
            jobs.add(scope.launch { gateway.audio.ttsCancelAll.collect { backend.cancelAll() } })
            jobs.add(scope.launch { gateway.audio.earcon.collect { (_, msg) -> backend.playEarcon(msg.name) } })
            jobs.add(scope.launch { gateway.audio.tts.collect { (_, msg) -> handleTts(msg, gateway) } })
        }
    }

    public suspend fun stop() {
        mutex.withLock { stopJobs() }
        runCatching { backend.cancelAll() }
    }

    public fun close() {
        scope.cancel()
    }

    private fun stopJobs() {
        for (job in jobs) job.cancel()
        jobs.clear()
    }

    private suspend fun broadcastVolume(gateway: BridgethingGateway, level: Float) {
        runCatching { gateway.audio.volumeChanged(VolumeChanged(level = level, muted = false)) }
    }

    private suspend fun handleTts(msg: Tts, gateway: BridgethingGateway) {
        val id = msg.id
        val completed = backend.speak(id, msg.text, msg.voice) {
            scope.launch { runCatching { gateway.audio.ttsStarted(TtsStarted(id)) } }
        }
        runCatching { gateway.audio.ttsEnded(TtsEnded(id, completed)) }
    }
}
