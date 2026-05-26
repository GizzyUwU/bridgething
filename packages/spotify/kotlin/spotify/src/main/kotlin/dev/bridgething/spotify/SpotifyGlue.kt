package dev.bridgething.spotify

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueCapability
import dev.bridgething.glue.GlueError
import dev.bridgething.schema.MusicProvider

/**
 * `BridgethingGlue` impl backed by a hand-ported Kotlin Spotify Web API +
 * dealer WS client.
 */
class SpotifyGlue(
    private val authenticator: SpotifyAuthenticator,
) : BridgethingGlue {
    override val name: String = "spotify"
    override val displayName: String = "Spotify"

    override val capabilities: Set<GlueCapability> = setOf(
        GlueCapability.STREAMING,
        GlueCapability.QUEUE,
        GlueCapability.ALBUM_ART,
        GlueCapability.RECOMMENDATIONS,
        GlueCapability.RECENTLY_PLAYED,
        GlueCapability.LIBRARY,
        GlueCapability.PLAYLISTS,
    )

    override val uriSchemes: List<String> = listOf("spotify")
    override val musicProvider: MusicProvider = MusicProvider.Spotify
    override val lyricsSupported: Boolean = true

    override suspend fun attach(gateway: BridgethingGateway) {
        throw GlueError.NotImplemented
    }

    override suspend fun detach() {}
}

/**
 * Authorization interface `SpotifyGlue` takes at construction; concrete
 * implementations provide WebView PKCE or device-code auth.
 */
interface SpotifyAuthenticator {
    suspend fun authorize(): TokenBundle
    suspend fun refreshAccessToken(refreshToken: String): TokenBundle
}

data class TokenBundle(
    val accessToken: String,
    val refreshToken: String?,
    val tokenType: String?,
    val expiresIn: Int?,
    val scope: String?,
)
