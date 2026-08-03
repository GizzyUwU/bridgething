package com.bridgething.companion

import com.bridgething.schema.NluPopularityFilter
import com.bridgething.schema.NluTargetType
import com.bridgething.spotify.SpotifyGlue
import com.bridgething.spotify.SpotifyVoiceResolver
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows
import uniffi.spotify.VoicePopularity as SpVoicePopularity
import uniffi.spotify.VoiceResolved as SpVoiceResolved
import uniffi.spotify.VoiceTargetKind as SpVoiceTargetKind

class SpotifyVoiceResolverTest {
    private class ResolveFailure : Exception("offline")

    private fun prediction(intent: String, slots: NluMutableSlots) =
        NluPrediction(intent = intent, transcript = "spoken", slots = slots)

    @Test
    fun `target and type map onto the request`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        val resolver = SpotifyVoiceResolver(fake)
        resolver.decorate(
            prediction("PLAY", NluMutableSlots(target = "  Hounds of Love ", targetType = NluTargetType.Album)),
        )
        assertEquals(1, fake.voiceResolveCalls.size)
        assertEquals(
            "Hounds of Love",
            fake.voiceResolveCalls.first().target,
            "surrounding whitespace is not part of the query",
        )
        assertEquals(SpVoiceTargetKind.ALBUM, fake.voiceResolveCalls.first().targetType)
    }

    @Test
    fun `podcast target type maps to show`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        SpotifyVoiceResolver(fake).decorate(
            prediction("PLAY", NluMutableSlots(target = "Reply All", targetType = NluTargetType.Podcast)),
        )
        assertEquals(SpVoiceTargetKind.SHOW, fake.voiceResolveCalls.first().targetType)
    }

    @Test
    fun `station target type survives to the request`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        SpotifyVoiceResolver(fake).decorate(
            prediction("PLAY", NluMutableSlots(target = "Kate Bush", targetType = NluTargetType.Station)),
        )
        assertEquals(SpVoiceTargetKind.STATION, fake.voiceResolveCalls.first().targetType)
    }

    @Test
    fun `mood genre and era travel as query terms`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        SpotifyVoiceResolver(fake).decorate(
            prediction("PLAY", NluMutableSlots(genre = "indie folk", mood = "chill", era = "80s")),
        )
        val req = fake.voiceResolveCalls.first()
        assertNull(req.target)
        assertEquals("chill", req.mood)
        assertEquals("indie folk", req.genre)
        assertEquals("80s", req.era)
    }

    @Test
    fun `position travels without a target`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        SpotifyVoiceResolver(fake).decorate(prediction("PLAY", NluMutableSlots(position = 3u)))
        assertEquals(3u, fake.voiceResolveCalls.first().position, "a position counts into whatever is playing")
    }

    @Test
    fun `random popularity alone still resolves`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        SpotifyVoiceResolver(fake).decorate(
            prediction("PLAY", NluMutableSlots(popularityFilter = NluPopularityFilter.Random)),
        )
        assertEquals(
            SpVoicePopularity.RANDOM,
            fake.voiceResolveCalls.first().popularityFilter,
            "\"play something\" is a fresh pick, not a resume",
        )
    }

    @Test
    fun `resolved uri and context uri land in slots`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        fake.voiceResolved = SpVoiceResolved(
            uri = "spotify:track:7", contextUri = "spotify:album:2", display = "Cloudbusting",
            kind = SpVoiceTargetKind.TRACK, alternatives = emptyList(),
        )
        val decorated = SpotifyVoiceResolver(fake).decorate(prediction("PLAY", NluMutableSlots(target = "cloudbusting")))
        assertEquals("spotify:track:7", decorated.slots.uri)
        assertEquals("spotify:album:2", decorated.slots.contextUri)
    }

    @Test
    fun `resolution without a context leaves context uri unset`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        val decorated = SpotifyVoiceResolver(fake).decorate(prediction("PLAY", NluMutableSlots(target = "mix")))
        assertEquals("spotify:playlist:1", decorated.slots.uri)
        assertNull(decorated.slots.contextUri)
    }

    @Test
    fun `catalog intents all resolve`() = runBlocking {
        for (intent in listOf("PLAY", "ADD_TO_QUEUE", "ADD_TO_PLAYLIST", "SEARCH", "THUMBS_UP")) {
            val fake = SpotifyGlueDispatchTest.FakeClient()
            val decorated = SpotifyVoiceResolver(fake).decorate(prediction(intent, NluMutableSlots(target = "mix")))
            assertEquals("spotify:playlist:1", decorated.slots.uri, "$intent names catalog content")
        }
    }

    @Test
    fun `non catalog intent passes through untouched`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        val original = prediction("SET_VOLUME", NluMutableSlots(target = "loud", level = 80u))
        val decorated = SpotifyVoiceResolver(fake).decorate(original)
        assertTrue(fake.voiceResolveCalls.isEmpty(), "a volume verb never names catalog content")
        assertEquals(original.slots, decorated.slots)
    }

    @Test
    fun `bare play resume never searches`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        val decorated = SpotifyVoiceResolver(fake).decorate(prediction("PLAY", NluMutableSlots()))
        assertTrue(fake.voiceResolveCalls.isEmpty(), "a bare resume has nothing to resolve")
        assertNull(decorated.slots.uri)
    }

    @Test
    fun `target type alone is not a request`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        SpotifyVoiceResolver(fake).decorate(prediction("PLAY", NluMutableSlots(targetType = NluTargetType.Album)))
        assertTrue(fake.voiceResolveCalls.isEmpty(), "a kind narrows a request, it cannot be one")
    }

    @Test
    fun `blank target is not a request`() = runBlocking {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        SpotifyVoiceResolver(fake).decorate(prediction("PLAY", NluMutableSlots(target = "   ")))
        assertTrue(fake.voiceResolveCalls.isEmpty())
    }

    @Test
    fun `resolver failure surfaces to the caller`() {
        val fake = SpotifyGlueDispatchTest.FakeClient()
        fake.voiceResolveFailure = ResolveFailure()
        assertThrows<ResolveFailure> {
            runBlocking { SpotifyVoiceResolver(fake).decorate(prediction("PLAY", NluMutableSlots(target = "mix"))) }
        }
    }

    @Test
    fun `glue exposes no resolver before attach`() {
        val glue = SpotifyGlue(
            workerBase = "https://example/auth",
            psk = "psk",
            deviceId = "dev",
            tokenStore = SpotifyGlueDispatchTest.FakeTokenStore("rt"),
            clientFactory = { _, _ -> SpotifyGlueDispatchTest.FakeClient() },
        )
        assertNull(glue.voiceResolver(), "an unattached glue holds no client to resolve through")
    }
}
