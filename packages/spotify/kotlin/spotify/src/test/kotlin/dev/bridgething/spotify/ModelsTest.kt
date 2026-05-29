package dev.bridgething.spotify

import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

class ModelsTest {
    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        coerceInputValues = true
    }

    @Test
    fun lossyListDropsUndecodableElements() {
        val text = """
            {"total":2,"items":[
              {"id":"1","uri":"spotify:track:1","name":"A"},
              "garbage",
              {"id":"2","uri":"spotify:track:2","name":"B"}
            ]}
        """.trimIndent()
        val page = json.decodeFromString(ItemsResponse.serializer(Track.serializer()), text)
        assertEquals(2, page.items.size)
        assertEquals(listOf("A", "B"), page.items.map { it.name })
    }

    @Test
    fun spotifyUriParseRoundTrips() {
        val uri = SpotifyUri.parse("spotify:track:abc123")
        assertEquals("abc123", uri?.id)
        assertEquals("spotify:track:abc123", uri?.string())
        assertNull(SpotifyUri.parse("not a uri"))
    }

    @Test
    fun artistRemapsShowEntries() {
        // spotify puts a podcast show in a playlist's artist slot with the show name in `type`.
        val text = """{"id":"s","uri":"spotify:show:s","name":"ignored","type":"Real Show Name"}"""
        val artist = json.decodeFromString(Artist.serializer(), text)
        assertEquals("Real Show Name", artist.name)
    }
}
