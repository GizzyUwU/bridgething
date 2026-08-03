package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.glue.AssetBytes
import com.bridgething.glue.BridgethingGlue
import com.bridgething.glue.GlueCapability
import com.bridgething.glue.GlueError
import com.bridgething.lyrics.Lyrics
import com.bridgething.lyrics.TrackIdentity
import com.bridgething.schema.BrowseResult
import com.bridgething.schema.FavoritesPage
import com.bridgething.schema.ItemRef
import com.bridgething.schema.LibraryBrowseRequest
import com.bridgething.schema.LibraryFavoritesContainsRequest
import com.bridgething.schema.LibraryFavoritesListRequest
import com.bridgething.schema.LibrarySearchRequest
import com.bridgething.schema.MusicProvider
import com.bridgething.schema.PlayUri
import com.bridgething.schema.SearchResult
import java.util.concurrent.CopyOnWriteArrayList

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
    override suspend fun handlePeerConnected(allowAutoResume: Boolean) { calls.add("peerConnected:$allowAutoResume") }

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
