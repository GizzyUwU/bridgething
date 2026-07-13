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

/**
 * Pluggable music-provider abstraction over a connected `BridgethingGateway`.
 *
 * Lifecycle-managed by `BridgethingCompanion`: `attach(gateway)` is called
 * after the gateway is running; inbound player verbs / asset / lyrics requests
 * are dispatched to the corresponding methods. Outbound now-playing is produced
 * by pushing snapshots/queue to the [NowPlayingSink] the companion injects via
 * [setNowPlayingSink]; the companion's hub is the sole emitter + authority
 * arbiter, so a glue never calls `gateway.player`/`gateway.authority` directly.
 *
 * Glues contribute `uriSchemes`, `musicProvider`, and `lyricsSupported`;
 * other capabilities (geo, net, audioTts, ...) are companion-level.
 * Default impls throw [GlueError.NotImplemented]; concrete glues override
 * what they support.
 */
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

    // Library surface. Default impls throw NotImplemented; the companion maps that to a
    // protocol `Unimplemented` reply (recognized verb, no backend) vs a domain LibraryError.
    suspend fun browse(req: LibraryBrowseRequest): BrowseResult = throw GlueError.NotImplemented
    suspend fun resolveContext(uri: String): ContextResolveReply = throw GlueError.NotImplemented
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
     * observer with deltas alongside its outbound now-playing snapshots; the
     * companion forwards these to the phone-side UI shell. `null` means
     * "nothing playing / source went away". Default impl is no-op for stub glues.
     */
    suspend fun setNowPlayingObserver(observer: (GlueNowPlaying?) -> Unit) {}

    /**
     * The companion injects the hub sink here before [attach]; `null` on detach.
     * Glues that produce now-playing push snapshots/queue to it instead of
     * calling the gateway, so the hub stays the sole emitter + authority
     * arbiter (the daemon cannot arbitrate two companion sources). Default no-op.
     */
    suspend fun setNowPlayingSink(sink: NowPlayingSink?) {}

    /**
     * Set the art render sizes (hero / thumb px) the active webapp declares, so the
     * glue warms art pushes at exactly what gets rendered. Default impl is no-op.
     */
    suspend fun setArtProfile(heroPx: Int, thumbPx: Int) {}

    /**
     * A device peer (re)connected. The daemon drops companion authority on disconnect, so the glue
     * resets its authority cache and re-emits current now-playing to re-establish it.
     * allowAutoResume permits the glue to aggressively resume playback for this connect. Default no-op.
     */
    suspend fun handlePeerConnected(allowAutoResume: Boolean) {}

    /** default no-op; glues with interactive sign-in drive this to surface auth transitions. */
    suspend fun setAuthObserver(observer: (GlueAuthState) -> Unit) {}

    /** default healthy; glues with a remote API drive this to surface degraded states. */
    suspend fun setServiceHealthObserver(observer: (GlueServiceHealth) -> Unit) {
        observer(GlueServiceHealth.Ok)
    }

    suspend fun ownsVolume(): Boolean = false
    suspend fun volumeUp(): Float = throw GlueError.NotImplemented
    suspend fun volumeDown(): Float = throw GlueError.NotImplemented
    suspend fun setVolume(level: Float): Float = throw GlueError.NotImplemented

    /** Live augmentation state for the debug surface. Default is all-false. */
    suspend fun debugState(): GlueDebugState = GlueDebugState()
}

/** Snapshot of a glue's now-playing augmentation, surfaced to the debug page. */
data class GlueDebugState(
    val authorityPlaybackHeld: Boolean = false,
    val authorityMetadataHeld: Boolean = false,
)

/** Auth-lifecycle state an interactive glue reports to the companion. */
sealed class GlueAuthState {
    data class Pending(val prompt: GlueDeviceCodePrompt?) : GlueAuthState()
    object Authenticated : GlueAuthState()
    data class Failed(val reason: String) : GlueAuthState()
}

/** Provider service health, surfaced alongside (not inside) auth state. */
sealed class GlueServiceHealth {
    object Ok : GlueServiceHealth()
    data class RateLimited(val retryAfterSeconds: Int) : GlueServiceHealth()
    object Unreachable : GlueServiceHealth()
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
