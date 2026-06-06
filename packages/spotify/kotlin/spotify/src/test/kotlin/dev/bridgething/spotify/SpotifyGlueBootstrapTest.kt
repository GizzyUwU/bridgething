package dev.bridgething.spotify

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.glue.GlueNowPlaying
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class SpotifyGlueBootstrapTest {
    private val playingState = """
        {"is_playing":true,"progress_ms":5000,"shuffle_state":false,"repeat_state":"off",
         "currently_playing_type":"track",
         "item":{"type":"track","id":"1","uri":"spotify:track:1","name":"NowSong","duration_ms":200000,
           "artists":[{"id":"a","uri":"spotify:artist:a","name":"Artist","type":"artist"}]}}
    """.trimIndent()

    @Test
    fun playerStateUpdateDrivesNowPlaying() = runBlocking {
        val engine = MockEngine { respond(content = "", status = HttpStatusCode.NotFound) }
        val glue = SpotifyGlue(
            authenticatorFactory = { StubAuthenticator() },
            accessToken = "token",
            refreshToken = "refresh",
            engine = engine,
        )

        val seen = CompletableDeferred<GlueNowPlaying>()
        glue.setNowPlayingObserver { np ->
            if (np != null && !seen.isCompleted) seen.complete(np)
        }

        val json = Json { ignoreUnknownKeys = true; isLenient = true; coerceInputValues = true }
        val pushed = json.decodeFromString(PlayerState.serializer(), playingState)

        try {
            glue.attach(BridgethingGateway(NoOpAdapter()))
            glue.playerStateUpdated(null, pushed)
            val nowPlaying = withTimeout(5_000) { seen.await() }
            assertEquals("NowSong", nowPlaying.update.mediaItem?.title)
        } finally {
            glue.detach()
        }
    }
}
