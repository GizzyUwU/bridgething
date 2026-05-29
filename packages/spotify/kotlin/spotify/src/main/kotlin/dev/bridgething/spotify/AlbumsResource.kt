package dev.bridgething.spotify

import kotlinx.serialization.Serializable
import kotlinx.serialization.serializer

class AlbumsResource(private val client: SpotinyClient) {

    suspend fun getAlbum(uri: SpotifyUri?): Album? {
        if (uri == null) return null

        val urlStr = "https://api.spotify.com/v1/albums/${uri.id}?fields=id%2Curi%2Cname%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29"

        return client.http.getJson<Album>(urlStr)
    }

    suspend fun getAlbumTracks(uri: SpotifyUri?, limit: Int? = null, offset: Int? = null): ItemsPage<Track> {
        if (uri == null) return ItemsPage.empty()

        val urlStr = "https://api.spotify.com/v1/albums/${uri.id}/tracks?fields=next%2Ctotal%2Citems%28id%2Curi%2Cname%2Cexplicit%2Cduration_ms%2Calbum%28id%2Curi%2Cname%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29%29%2Cartists%28id%2Curi%2Cname%2Ctype%29%29"

        return client.http.getItems<Track>(urlStr, limit = limit, offset = offset)
    }

    suspend fun getUserSavedAlbums(limit: Int? = null, offset: Int? = null): ItemsPage<Album> {
        val urlStr = "https://api.spotify.com/v1/me/albums?fields=next%2Ctotal%2Citems%28album%28id%2Curi%2Cname%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29%29%29"

        val page = client.http.getMany<ItemsResponse<SavedAlbum>, SavedAlbum>(
            url = urlStr,
            deserializer = serializer<ItemsResponse<SavedAlbum>>(),
            extract = { Triple(it.next, it.total, it.items) },
            limit = limit,
            offset = offset,
        )

        return page.map { it.album }
    }

    @Serializable
    private data class SavedAlbum(val album: Album = Album())
}
