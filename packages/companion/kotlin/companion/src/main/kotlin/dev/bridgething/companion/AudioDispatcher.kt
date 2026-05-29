package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.audio
import dev.bridgething.schema.Tts
import dev.bridgething.schema.TtsEnded
import dev.bridgething.schema.TtsStarted
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

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            stopJobs()
            jobs.add(scope.launch { gateway.audio.volumeUp.collect { backend.volumeUp() } })
            jobs.add(scope.launch { gateway.audio.volumeDown.collect { backend.volumeDown() } })
            jobs.add(scope.launch { gateway.audio.setVolume.collect { (_, msg) -> backend.setVolume(msg.level) } })
            jobs.add(scope.launch { gateway.audio.muteToggle.collect { backend.muteToggle() } })
            jobs.add(scope.launch { gateway.audio.setMute.collect { (_, msg) -> backend.setMute(msg.muted) } })
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

    private suspend fun handleTts(msg: Tts, gateway: BridgethingGateway) {
        val id = msg.id
        val completed = backend.speak(id, msg.text, msg.voice) {
            scope.launch { runCatching { gateway.audio.ttsStarted(TtsStarted(id)) } }
        }
        runCatching { gateway.audio.ttsEnded(TtsEnded(id, completed)) }
    }
}
