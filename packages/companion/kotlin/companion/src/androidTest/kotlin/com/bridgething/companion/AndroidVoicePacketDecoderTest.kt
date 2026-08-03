package com.bridgething.companion

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidVoicePacketDecoderTest {
    private val decoder = AndroidVoicePacketDecoder()

    @Test
    fun realDecodeLandsOnTheStreamsOwnRate() = runBlocking {
        val samples = decoder.decode(VoiceOpusFixture.packets, VoiceOpusFixture.format)
        val expected = VoiceOpusFixture.packets.size * VoiceOpusFixture.SAMPLES_PER_PACKET
        assertTrue(
            "decoded ${samples.size} samples, expected about $expected at ${VoiceOpusFixture.SAMPLE_RATE_HZ} hz",
            samples.size > expected - 400 && samples.size < expected + 400,
        )
    }

    @Test
    fun realDecodePreservesTheTone() = runBlocking {
        val samples = decoder.decode(VoiceOpusFixture.packets, VoiceOpusFixture.format)
        val tone = VoiceOpusFixture.energy(samples, VoiceOpusFixture.TONE_HZ)
        val elsewhere = VoiceOpusFixture.energy(samples, 1500.0)
        assertTrue(
            "440 hz carried $tone, 1500 hz carried $elsewhere",
            tone > elsewhere * 100,
        )
    }

    @Test
    fun anEmptyTurnDecodesToNothing() = runBlocking {
        assertEquals(0, decoder.decode(emptyList(), VoiceOpusFixture.format).size)
    }
}
