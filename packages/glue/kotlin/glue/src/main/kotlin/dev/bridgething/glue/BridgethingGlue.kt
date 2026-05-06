package dev.bridgething.glue

import dev.bridgething.gateway.BridgethingGateway

/**
 * Pluggable music-provider abstraction over a connected `BridgethingGateway`.
 * One concrete impl per music service (Spotify, Apple Music, Tidal, ...).
 * At most one glue is attached to a gateway at a time; switching providers
 * goes detach -> attach.
 */
interface BridgethingGlue {
    val name: String
    val displayName: String
    val capabilities: Set<GlueCapability>

    suspend fun attach(gateway: BridgethingGateway)
    suspend fun detach()
}

enum class GlueCapability {
    STREAMING,
    QUEUE,
    LYRICS,
    ALBUM_ART,
    RECOMMENDATIONS,
    RECENTLY_PLAYED,
    LIBRARY,
    PLAYLISTS,
}

sealed class GlueError(message: String, cause: Throwable? = null) : Exception(message, cause) {
    object NotImplemented : GlueError("not implemented")
    object NotAuthenticated : GlueError("not authenticated")
    object Detached : GlueError("glue is detached")
    class Underlying(cause: Throwable) : GlueError(cause.message ?: "underlying error", cause)
}
