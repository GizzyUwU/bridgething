package dev.bridgething.glue

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.lyrics.Lyrics
import dev.bridgething.lyrics.TrackIdentity
import dev.bridgething.schema.BrowseResult
import dev.bridgething.schema.FavoritesPage
import dev.bridgething.schema.FavoritesSet
import dev.bridgething.schema.ItemRef
import dev.bridgething.schema.LibraryBrowseRequest
import dev.bridgething.schema.LibraryFavoritesContainsRequest
import dev.bridgething.schema.LibraryFavoritesListRequest
import dev.bridgething.schema.LibraryRecommendationsRequest
import dev.bridgething.schema.LibrarySearchRequest
import dev.bridgething.schema.MusicProvider
import dev.bridgething.schema.NowPlayingUpdate
import dev.bridgething.schema.PlayUri
import dev.bridgething.schema.RecommendationsResult
import dev.bridgething.schema.RepeatMode
import dev.bridgething.schema.SearchResult

/**
 * Pluggable music-provider abstraction over a connected `BridgethingGateway`.
 *
 * Lifecycle-managed by `BridgethingCompanion`: `attach(gateway)` is called
 * after the gateway is running; inbound player verbs / asset / lyrics requests
 * are dispatched to the corresponding methods. Outbound events (NowPlayingUpdate
 * deltas, authority claim/release) are the glue's own responsibility.
 *
 * Glues contribute `uriSchemes`, `musicProvider`, and `lyricsSupported`;
 * other capabilities (geo, net, audioTts, ...) are companion-level.
 * Default impls throw [GlueError.NotImplemented]; concrete glues override
 * what they support.
 */
interface BridgethingGlue {
    val name: String
    val displayName: String
    val capabilities: Set<GlueCapability>
    val uriSchemes: List<String>
    val musicProvider: MusicProvider
    val lyricsSupported: Boolean

    suspend fun attach(gateway: BridgethingGateway)
    suspend fun detach()

    suspend fun play(uri: PlayUri): Unit = throw GlueError.NotImplemented
    suspend fun pause(): Unit = throw GlueError.NotImplemented
    suspend fun resume(): Unit = throw GlueError.NotImplemented
    suspend fun skipNext(): Unit = throw GlueError.NotImplemented
    suspend fun skipPrev(): Unit = throw GlueError.NotImplemented
    suspend fun skipToIndex(index: UInt): Unit = throw GlueError.NotImplemented
    suspend fun seekTo(positionMs: UInt): Unit = throw GlueError.NotImplemented
    suspend fun setShuffle(on: Boolean): Unit = throw GlueError.NotImplemented
    suspend fun setRepeat(mode: RepeatMode): Unit = throw GlueError.NotImplemented
    suspend fun setSpeed(speed: Float): Unit = throw GlueError.NotImplemented
    suspend fun setCrossfade(durationMs: UInt?): Unit = throw GlueError.NotImplemented

    // Library surface. Default impls throw NotImplemented; the companion maps that to a
    // protocol `Unimplemented` reply (recognized verb, no backend) vs a domain LibraryError.
    suspend fun browse(req: LibraryBrowseRequest): BrowseResult = throw GlueError.NotImplemented
    suspend fun search(req: LibrarySearchRequest): SearchResult = throw GlueError.NotImplemented
    suspend fun recommendations(req: LibraryRecommendationsRequest): RecommendationsResult = throw GlueError.NotImplemented
    suspend fun favoritesList(req: LibraryFavoritesListRequest): FavoritesPage = throw GlueError.NotImplemented
    suspend fun favoritesContains(req: LibraryFavoritesContainsRequest): List<Boolean> = throw GlueError.NotImplemented
    suspend fun favoritesToggle(item: ItemRef): Unit = throw GlueError.NotImplemented
    suspend fun favoritesSet(item: ItemRef, liked: Boolean): Unit = throw GlueError.NotImplemented
    suspend fun favoritesSetMany(entries: List<FavoritesSet>): Unit = throw GlueError.NotImplemented

    /**
     * Bytes for an asset id this glue produced (e.g.
     * `"spotify/img/<percent-encoded>"`). Return null if the id isn't this
     * glue's; the companion replies `AssetNotFound` in that case.
     */
    suspend fun asset(id: String): AssetBytes? = null

    /**
     * Provider-native lyrics path. Return null to fall through to the
     * companion's injected `LyricsResolver` (lrclib by default).
     */
    suspend fun lyrics(track: TrackIdentity): Lyrics? = null

    /**
     * Subscribe to NowPlaying mirror updates. The active glue invokes the
     * observer with deltas alongside its outbound `gateway.player.delta`
     * events; the companion forwards these to the phone-side UI shell.
     * `null` means "nothing playing / source went away". Default impl is
     * no-op for stub glues.
     */
    suspend fun setNowPlayingObserver(observer: (GlueNowPlaying?) -> Unit) {}

    /** default no-op; glues with interactive sign-in drive this to surface auth transitions. */
    suspend fun setAuthObserver(observer: (GlueAuthState) -> Unit) {}
}

/** Auth-lifecycle state an interactive glue reports to the companion. */
sealed class GlueAuthState {
    data class Pending(val prompt: GlueDeviceCodePrompt?) : GlueAuthState()
    object Authenticated : GlueAuthState()
    data class Failed(val reason: String) : GlueAuthState()
}

/** RFC 8628 device-code prompt the user completes in a browser. */
data class GlueDeviceCodePrompt(
    val userCode: String,
    val verificationUrl: String,
    val verificationUrlComplete: String?,
)

/**
 * NowPlaying snapshot the active glue surfaces to the companion. Wraps
 * the wire `NowPlayingUpdate` with the raw artwork URL so phone-side UI
 * can load directly from the provider's CDN, bypassing the on-device
 * asset-cache indirection.
 */
data class GlueNowPlaying(
    val update: NowPlayingUpdate,
    val artworkUrl: String? = null,
)

/** Bytes payload returned from `BridgethingGlue.asset(id)`. */
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
