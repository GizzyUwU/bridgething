package dev.bridgething.spotify

import kotlinx.serialization.Serializable

class PlaylistsResource(private val client: SpotinyClient) {
    suspend fun getPlaylist(uri: SpotifyUri?): Playlist? {
        if (uri == null) return null

        val urlStr =
            "https://api.spotify.com/v1/playlists/${uri.id}?fields=id%2Curi%2Cname%2Cdescription%2Cimages"

        return client.http.getJson<Playlist>(urlStr)
    }

    suspend fun getPlaylistItems(
        uri: SpotifyUri?,
        limit: Int? = null,
        offset: Int? = null,
    ): ItemsPage<PlaylistItem> {
        if (uri == null) return ItemsPage.empty()

        val urlStr =
            "https://api.spotify.com/v1/playlists/${uri.id}/items?fields=next%2Ctotal%2Citems%28item%28type%2Cid%2Curi%2Cname%2Cis_playable%2Cexplicit%2Cduration_ms%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29%2Calbum%28id%2Curi%2Cname%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29%29%29%29"

        val page = client.http.getItems<PlaylistItemResponse>(urlStr, limit = limit, offset = offset)

        return page.map { it.item }
    }

    suspend fun getUserPlaylists(
        limit: Int? = null,
        offset: Int? = null,
    ): ItemsPage<Playlist> {
        val urlStr =
            "https://api.spotify.com/v1/me/playlists?fields=next%2Ctotal%2Citems%28id%2Curi%2Cname%2Cdescription%2Cimages%29"

        return client.http.getItems<Playlist>(urlStr, limit = limit, offset = offset)
    }
}

@Serializable
private data class PlaylistItemResponse(val item: PlaylistItem = PlaylistItem())
