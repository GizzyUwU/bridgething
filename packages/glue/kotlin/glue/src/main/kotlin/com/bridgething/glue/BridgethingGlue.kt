package com.bridgething.glue

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.lyrics.Lyrics
import com.bridgething.lyrics.TrackIdentity
import com.bridgething.schema.BrowseResult
import com.bridgething.schema.ContextResolveReply
import com.bridgething.schema.FavoritesPage
import com.bridgething.schema.FavoritesSet
import com.bridgething.schema.ItemRef
import com.bridgething.schema.LibraryBrowseRequest
import com.bridgething.schema.LibraryFavoritesContainsRequest
import com.bridgething.schema.LibraryFavoritesListRequest
import com.bridgething.schema.LibraryRecommendationsRequest
import com.bridgething.schema.LibrarySearchRequest
import com.bridgething.schema.MusicProvider
import com.bridgething.schema.NowPlayingUpdate
import com.bridgething.schema.RecommendationsResult
import com.bridgething.schema.SearchResult

interface BridgethingGlue : NowPlayingTransport {
    val name: String
    val displayName: String
    val capabilities: Set<GlueCapability>
    val uriSchemes: List<String>
    val musicProvider: MusicProvider
    val lyricsSupported: Boolean

    val appBundles: List<String> get() = emptyList()

    suspend fun attach(gateway: BridgethingGateway)
    suspend fun detach()

    suspend fun browse(req: LibraryBrowseRequest): BrowseResult = throw GlueError.NotImplemented
    suspend fun resolveContext(uri: String): ContextResolveReply = throw GlueError.NotImplemented
    suspend fun search(req: LibrarySearchRequest): SearchResult = throw GlueError.NotImplemented
    suspend fun recommendations(req: LibraryRecommendationsRequest): RecommendationsResult = throw GlueError.NotImplemented
    suspend fun favoritesList(req: LibraryFavoritesListRequest): FavoritesPage = throw GlueError.NotImplemented
    suspend fun favoritesContains(req: LibraryFavoritesContainsRequest): List<Boolean> = throw GlueError.NotImplemented
    suspend fun favoritesToggle(item: ItemRef): Unit = throw GlueError.NotImplemented
    suspend fun favoritesSet(item: ItemRef, liked: Boolean): Unit = throw GlueError.NotImplemented
    suspend fun favoritesSetMany(entries: List<FavoritesSet>): Unit = throw GlueError.NotImplemented

    suspend fun asset(id: String): AssetBytes? = null
    suspend fun lyrics(track: TrackIdentity): Lyrics? = null
    suspend fun setNowPlayingObserver(observer: (GlueNowPlaying?) -> Unit) {}
    suspend fun setNowPlayingSink(sink: NowPlayingSink?) {}
    suspend fun setArtProfile(heroPx: Int, thumbPx: Int) {}
    suspend fun handlePeerConnected(allowAutoResume: Boolean) {}
    suspend fun setAuthObserver(observer: (GlueAuthState) -> Unit) {}
    suspend fun setServiceHealthObserver(observer: (GlueServiceHealth) -> Unit) {
        observer(GlueServiceHealth.Ok)
    }

    suspend fun ownsVolume(): Boolean = false
    suspend fun volumeUp(): Float = throw GlueError.NotImplemented
    suspend fun volumeDown(): Float = throw GlueError.NotImplemented
    suspend fun setVolume(level: Float): Float = throw GlueError.NotImplemented

    val supportsPlaybackTargets: Boolean get() = false

    suspend fun transferTo(targetId: String): Unit = throw GlueError.NotImplemented

    suspend fun debugState(): GlueDebugState = GlueDebugState()
}

data class GlueDebugState(
    val authorityPlaybackHeld: Boolean = false,
    val authorityMetadataHeld: Boolean = false,
)

sealed class GlueAuthState {
    data class Pending(val prompt: GlueDeviceCodePrompt?) : GlueAuthState()
    object Authenticated : GlueAuthState()
    data class Failed(val reason: String) : GlueAuthState()
}

sealed class GlueServiceHealth {
    object Ok : GlueServiceHealth()
    data class RateLimited(val retryAfterSeconds: Int) : GlueServiceHealth()
    object Unreachable : GlueServiceHealth()
}

data class GlueDeviceCodePrompt(
    val userCode: String,
    val verificationUrl: String,
    val verificationUrlComplete: String?,
)

data class GlueNowPlaying(
    val update: NowPlayingUpdate,
    val artworkUrl: String? = null,
)

data class AssetBytes(
    val bytes: ByteArray,
    val mime: String? = null,
)

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
