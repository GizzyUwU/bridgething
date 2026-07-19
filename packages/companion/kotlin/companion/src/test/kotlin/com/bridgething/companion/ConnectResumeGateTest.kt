package com.bridgething.companion

import com.bridgething.gateway.AdapterEvent
import com.bridgething.gateway.Device
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class ConnectResumeGateTest {
    private val device = Device(id = "carthing-1", name = "Bridgething")

    private suspend fun boot(
        scope: CoroutineScope,
        glue: FakeGlue,
        cooldownMs: Long = 300_000L,
    ): Pair<BridgethingCompanion, FakeAdapter> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
        )
        companion.autoResumeCooldownMs = cooldownMs
        companion.setActive(glue)
        companion.start()
        return companion to adapter
    }

    private suspend fun peerConnects(glue: FakeGlue, atLeast: Int): List<Boolean> {
        repeat(200) {
            val seen = glue.calls.filter { it.startsWith("peerConnected:") }
                .map { it.removePrefix("peerConnected:").toBoolean() }
            if (seen.size >= atLeast) return seen
            delay(10)
        }
        return glue.calls.filter { it.startsWith("peerConnected:") }
            .map { it.removePrefix("peerConnected:").toBoolean() }
    }

    @Test
    fun `reconnect soon after a drop still resumes once the cooldown has elapsed`() = runBlocking {
        val glue = FakeGlue()
        val (companion, adapter) = boot(this, glue, cooldownMs = 50L)

        adapter.simulate(AdapterEvent.Connected(device))
        assertEquals(listOf(true), peerConnects(glue, 1), "first connect resumes")

        delay(80)
        adapter.simulate(AdapterEvent.Disconnected(device.id))
        adapter.simulate(AdapterEvent.Connected(device))

        assertEquals(
            listOf(true, true),
            peerConnects(glue, 2),
            "the drop is recent but the last resume is not, so this connect must resume",
        )
        companion.stop()
    }

    @Test
    fun `second connect inside the cooldown does not resume again`() = runBlocking {
        val glue = FakeGlue()
        val (companion, adapter) = boot(this, glue)

        adapter.simulate(AdapterEvent.Connected(device))
        assertEquals(listOf(true), peerConnects(glue, 1), "first connect resumes")

        adapter.simulate(AdapterEvent.Disconnected(device.id))
        adapter.simulate(AdapterEvent.Connected(device))

        assertEquals(
            listOf(true, false),
            peerConnects(glue, 2),
            "a re-dial inside the cooldown must not resume a second time",
        )
        companion.stop()
    }

    @Test
    fun `disabled device never resumes`() = runBlocking {
        val glue = FakeGlue()
        val (companion, adapter) = boot(this, glue)
        companion.setDeviceAutoResume(device.id, false)

        adapter.simulate(AdapterEvent.Connected(device))

        assertEquals(listOf(false), peerConnects(glue, 1), "auto-resume off must veto regardless of timing")
        companion.stop()
    }
}
