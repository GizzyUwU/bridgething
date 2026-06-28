package com.bridgething.companion

import com.bridgething.schema.BridgeToGatewayAudioMsg
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.Earcon
import com.bridgething.schema.GatewayToBridgeAudioMsg
import com.bridgething.schema.GatewayToBridgeMsgData
import com.bridgething.schema.SetMute
import com.bridgething.schema.SetVolume
import com.bridgething.schema.Tts
import com.bridgething.schema.TtsCancel
import io.mockk.mockk
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.seconds

/** audio dispatch: verifies each inbound verb reaches the [AudioBackend] with the right args and that TTS emits the started/ended wire lifecycle. */
class AudioDispatchTest {
    private suspend fun boot(scope: CoroutineScope, backend: AudioBackend): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "audio-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = backend,
        )
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    @Test
    fun `volume and mute verbs route to backend`() = runBlocking {
        val backend = FakeAudioBackend()
        val (companion, driver) = boot(this, backend)
        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.SetVolume(SetVolume(level = 0.42f))))
        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.SetMute(SetMute(muted = true))))
        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.VolumeUp))
        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.MuteToggle))
        eventually {
            backend.setVolumeCalls == listOf(0.42f) &&
                backend.setMuteCalls == listOf(true) &&
                backend.volumeUpCount.get() == 1 &&
                backend.muteToggleCount.get() == 1
        }
        companion.stop()
    }

    @Test
    fun `tts emits started and ended completed`() = runBlocking {
        val backend = FakeAudioBackend()
        val (companion, driver) = boot(this, backend)
        val id = UUID.randomUUID()
        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.Tts(Tts(id = id, text = "hello", voice = null))))

        withTimeout(5.seconds) {
            driver.waitOutbound { msg ->
                val inner = (msg.data as? GatewayToBridgeMsgData.Audio)?.data as? GatewayToBridgeAudioMsg.TtsStarted
                inner?.data?.id == id
            }
        }
        val ended = withTimeout(5.seconds) {
            driver.waitOutbound { msg ->
                val inner = (msg.data as? GatewayToBridgeMsgData.Audio)?.data as? GatewayToBridgeAudioMsg.TtsEnded
                inner?.data?.id == id
            }
        }
        val info = ((ended.data as GatewayToBridgeMsgData.Audio).data as GatewayToBridgeAudioMsg.TtsEnded).data
        assertTrue(info.completed, "uncancelled speech should end completed")
        companion.stop()
    }

    @Test
    fun `tts cancel ends incomplete`() = runBlocking {
        val backend = FakeAudioBackend(blockUntilCancel = true)
        val (companion, driver) = boot(this, backend)
        val id = UUID.randomUUID()
        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.Tts(Tts(id = id, text = "long sentence", voice = null))))
        withTimeout(5.seconds) {
            driver.waitOutbound { msg ->
                val inner = (msg.data as? GatewayToBridgeMsgData.Audio)?.data as? GatewayToBridgeAudioMsg.TtsStarted
                inner?.data?.id == id
            }
        }

        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.TtsCancel(TtsCancel(id = id))))
        val ended = withTimeout(5.seconds) {
            driver.waitOutbound { msg ->
                val inner = (msg.data as? GatewayToBridgeMsgData.Audio)?.data as? GatewayToBridgeAudioMsg.TtsEnded
                inner?.data?.id == id
            }
        }
        val info = ((ended.data as GatewayToBridgeMsgData.Audio).data as GatewayToBridgeAudioMsg.TtsEnded).data
        assertFalse(info.completed, "cancelled speech should end not-completed")
        companion.stop()
    }

    @Test
    fun `earcon routes to backend`() = runBlocking {
        val backend = FakeAudioBackend()
        val (companion, driver) = boot(this, backend)
        driver.send(BridgeToGatewayMsgData.Audio(BridgeToGatewayAudioMsg.Earcon(Earcon(name = "confirm"))))
        eventually { backend.earconNames == listOf("confirm") }
        companion.stop()
    }

    private suspend fun eventually(predicate: () -> Boolean) {
        repeat(300) {
            if (predicate()) return
            delay(10)
        }
        assertEquals(true, predicate(), "predicate did not hold within the deadline")
    }
}

/** fake [AudioBackend] that records calls; [blockUntilCancel] keeps TTS in-flight until cancel so the cancel path is exercisable. */
class FakeAudioBackend(private val blockUntilCancel: Boolean = false) : AudioBackend {
    val setVolumeCalls = CopyOnWriteArrayList<Float>()
    val setMuteCalls = CopyOnWriteArrayList<Boolean>()
    val volumeUpCount = AtomicInteger(0)
    val volumeDownCount = AtomicInteger(0)
    val muteToggleCount = AtomicInteger(0)
    val earconNames = CopyOnWriteArrayList<String>()

    private val pending = ConcurrentHashMap<UUID, CompletableDeferred<Boolean>>()

    override suspend fun setVolume(level: Float) { setVolumeCalls.add(level) }
    override suspend fun setMute(muted: Boolean) { setMuteCalls.add(muted) }
    override suspend fun volumeUp() { volumeUpCount.incrementAndGet() }
    override suspend fun volumeDown() { volumeDownCount.incrementAndGet() }
    override suspend fun muteToggle() { muteToggleCount.incrementAndGet() }

    override suspend fun speak(id: UUID, text: String, voice: String?, onStart: () -> Unit): Boolean {
        onStart()
        if (!blockUntilCancel) return true
        val deferred = CompletableDeferred<Boolean>()
        pending[id] = deferred
        return deferred.await()
    }

    override suspend fun cancel(id: UUID) { pending.remove(id)?.complete(false) }
    override suspend fun cancelAll() {
        pending.values.forEach { it.complete(false) }
        pending.clear()
    }

    override suspend fun playEarcon(name: String): Boolean {
        earconNames.add(name)
        return false
    }
}
