package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.glue.AssetBytes
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueCapability
import dev.bridgething.glue.GlueError
import dev.bridgething.lyrics.Lyrics
import dev.bridgething.lyrics.TrackIdentity
import dev.bridgething.schema.BrowseResult
import dev.bridgething.schema.FavoritesPage
import dev.bridgething.schema.ItemRef
import dev.bridgething.schema.LibraryBrowseRequest
import dev.bridgething.schema.LibraryFavoritesContainsRequest
import dev.bridgething.schema.LibraryFavoritesListRequest
import dev.bridgething.schema.LibrarySearchRequest
import dev.bridgething.schema.MusicProvider
import dev.bridgething.schema.PlayUri
import dev.bridgething.schema.SearchResult
import java.util.concurrent.CopyOnWriteArrayList

/**
 * A [BridgethingGlue] with no real provider: records every dispatched verb and
 * returns canned data via the constructor closures. Used by the dispatch-layer
 * tests to verify routing + encoding, not Spotify. Library verbs with no closure
 * throw `NotImplemented` (exercising the protocol-error path); asset/lyrics with
 * no closure return null (exercising the not-found / resolver-fallthrough paths).
 */
class FakeGlue(
    override val name: String = "fake",
    override val displayName: String = "Fake",
    override val capabilities: Set<GlueCapability> =
        setOf(GlueCapability.STREAMING, GlueCapability.LIBRARY, GlueCapability.PLAYLISTS),
    override val uriSchemes: List<String> = listOf("fake"),
    override val musicProvider: MusicProvider = MusicProvider.None,
    override val lyricsSupported: Boolean = false,
    private val onBrowse: (suspend (LibraryBrowseRequest) -> BrowseResult)? = null,
    private val onSearch: (suspend (LibrarySearchRequest) -> SearchResult)? = null,
    private val onFavoritesList: (suspend (LibraryFavoritesListRequest) -> FavoritesPage)? = null,
    private val onFavoritesContains: (suspend (LibraryFavoritesContainsRequest) -> List<Boolean>)? = null,
    private val onAsset: (suspend (String) -> AssetBytes?)? = null,
    private val onLyrics: (suspend (TrackIdentity) -> Lyrics?)? = null,
) : BridgethingGlue {
    val calls = CopyOnWriteArrayList<String>()

    override suspend fun attach(gateway: BridgethingGateway) { calls.add("attach") }
    override suspend fun detach() { calls.add("detach") }

    override suspend fun play(uri: PlayUri) { calls.add("play:${uri.uri}") }
    override suspend fun pause() { calls.add("pause") }
    override suspend fun resume() { calls.add("resume") }
    override suspend fun skipNext() { calls.add("skipNext") }
    override suspend fun skipPrev() { calls.add("skipPrev") }

    override suspend fun browse(req: LibraryBrowseRequest): BrowseResult {
        calls.add("browse")
        return onBrowse?.invoke(req) ?: throw GlueError.NotImplemented
    }

    override suspend fun search(req: LibrarySearchRequest): SearchResult {
        calls.add("search:${req.query}")
        return onSearch?.invoke(req) ?: throw GlueError.NotImplemented
    }

    override suspend fun favoritesList(req: LibraryFavoritesListRequest): FavoritesPage {
        calls.add("favoritesList")
        return onFavoritesList?.invoke(req) ?: throw GlueError.NotImplemented
    }

    override suspend fun favoritesContains(req: LibraryFavoritesContainsRequest): List<Boolean> {
        calls.add("favoritesContains")
        return onFavoritesContains?.invoke(req) ?: throw GlueError.NotImplemented
    }

    override suspend fun favoritesToggle(item: ItemRef) { calls.add("favoritesToggle:${item.uri}") }

    override suspend fun asset(id: String): AssetBytes? {
        calls.add("asset:$id")
        return onAsset?.invoke(id)
    }

    override suspend fun lyrics(track: TrackIdentity): Lyrics? {
        calls.add("lyrics")
        return onLyrics?.invoke(track)
    }
}
