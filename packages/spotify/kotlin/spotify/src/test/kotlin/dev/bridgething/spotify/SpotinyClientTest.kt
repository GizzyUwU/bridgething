package dev.bridgething.spotify

import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

private class FakeAuthenticator : SpotifyAuthenticator {
    override suspend fun authorize(): TokenBundle = TokenBundle("t", "r", "Bearer", 3600, null)
    override suspend fun refreshAccessToken(refreshToken: String): TokenBundle =
        TokenBundle("t", "r", "Bearer", 3600, null)
}

private fun clientReturning(body: String): SpotinyClient {
    val engine = MockEngine { respond(content = body, status = HttpStatusCode.OK) }
    return SpotinyClient(FakeAuthenticator(), accessToken = "token", engine = engine)
}

class SpotinyClientTest {
    @Test
    fun searchReturnsMappedTracks() = runTest {
        val body = """
            {"tracks":{"total":1,"items":[
              {"id":"1","uri":"spotify:track:1","name":"Song","duration_ms":1000,
               "artists":[{"id":"a","uri":"spotify:artist:a","name":"Artist","type":"artist"}],
               "album":{"id":"al","uri":"spotify:album:al","name":"Album",
                 "images":[{"url":"http://img","height":640,"width":640}]}}
            ]}}
        """.trimIndent()
        val results = clientReturning(body).search.search("song", listOf("track"), 20, 0)
        assertEquals(1, results.tracks.size)
        assertEquals("Song", results.tracks[0].name)
        assertEquals("Artist", results.tracks[0].subtitle)
    }

    @Test
    fun lossyDecodeDropsMalformedSavedTrack() = runTest {
        // middle element is a bare string, not a saved-track object; it must drop, not fail the page.
        val body = """
            {"total":3,"items":[
              {"track":{"id":"1","uri":"spotify:track:1","name":"A"}},
              "garbage",
              {"track":{"id":"2","uri":"spotify:track:2","name":"B"}}
            ]}
        """.trimIndent()
        val page = clientReturning(body).tracks.getUserSavedTracks()
        assertEquals(2, page.items.size)
        assertEquals(listOf("A", "B"), page.items.map { it.name })
    }

    @Test
    fun playbackStateDecodesTrackItem() = runTest {
        val body = """
            {"is_playing":true,"progress_ms":5000,"shuffle_state":false,"repeat_state":"off",
             "currently_playing_type":"track",
             "item":{"type":"track","id":"1","uri":"spotify:track:1","name":"NowSong","duration_ms":200000,
               "artists":[{"id":"a","uri":"spotify:artist:a","name":"Artist","type":"artist"}]}}
        """.trimIndent()
        val state = clientReturning(body).player.getPlaybackState()
        assertTrue(state != null)
        assertTrue(state!!.isPlaying)
        val item = state.item
        assertTrue(item is PlayerItem.TrackItem)
        item as PlayerItem.TrackItem
        assertEquals("NowSong", item.name)
    }

    @Test
    fun playbackStateDecodesEpisodeItem() = runTest {
        val body = """
            {"is_playing":true,"progress_ms":0,"currently_playing_type":"episode",
             "item":{"type":"episode","id":"e","uri":"spotify:episode:e","name":"Ep1","duration_ms":300000,
               "show":{"id":"s","uri":"spotify:show:s","name":"Show1","publisher":"Pub"}}}
        """.trimIndent()
        val state = clientReturning(body).player.getPlaybackState()
        val item = state?.item
        assertTrue(item is PlayerItem.EpisodeItem)
        item as PlayerItem.EpisodeItem
        assertEquals("Ep1", item.name)
        assertEquals("Show1", item.subtitle)
    }
}
