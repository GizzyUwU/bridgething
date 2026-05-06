package dev.bridgething.spotify

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueCapability
import dev.bridgething.glue.GlueError

/**
 * `BridgethingGlue` impl backed by a hand-ported Kotlin Spotify Web API +
 * dealer WS client (mirror of spotiny on the Swift side). `attach` body
 * lands in a follow-up slice; the type, capabilities, and constructor
 * shape are stable from this round.
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

    override suspend fun attach(gateway: BridgethingGateway) {
        throw GlueError.NotImplemented
    }

    override suspend fun detach() {
    }
}

/**
 * Mirror of spotiny's `OAuthAuthenticator` Swift protocol. Two impls
 * (WebView PKCE and device-code) land in the next slice; the surface
 * is here so SpotifyGlue can take one at construction.
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
