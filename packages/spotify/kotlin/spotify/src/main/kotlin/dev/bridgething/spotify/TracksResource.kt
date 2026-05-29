package dev.bridgething.spotify

import kotlinx.serialization.Serializable

class TracksResource(private val client: SpotinyClient) {
    suspend fun getTrack(uri: SpotifyUri?): Track? {
        if (uri == null) return null

        val urlStr =
            "https://api.spotify.com/v1/tracks/${uri.id}?fields=id%2Curi%2Cname%2Cexplicit%2Cduration_ms%2Calbum%28id%2Curi%2Cname%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29%29%2Cartists%28id%2Curi%2Cname%2Ctype%29"

        return client.http.getJson<Track>(urlStr)
    }

    suspend fun getUserSavedTracks(limit: Int? = null, offset: Int? = null): ItemsPage<Track> {
        val urlStr =
            "https://api.spotify.com/v1/me/tracks?fields=next%2Ctotal%2Citems%28track%28id%2Curi%2Cname%2Cexplicit%2Cduration_ms%2Calbum%28id%2Curi%2Cname%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29%29%2Cartists%28id%2Curi%2Cname%2Ctype%29%29%29"

        val page = client.http.getItems<SavedTrack>(urlStr, limit = limit, offset = offset)

        return page.map { it.track }
    }

    suspend fun getUserTopTracks(limit: Int? = null, offset: Int? = null): ItemsPage<Track> {
        val urlStr = "https://api.spotify.com/v1/me/top/tracks?time_range=medium_term"

        return client.http.getItems<Track>(urlStr, limit = limit, offset = offset)
    }
}

@Serializable
private data class SavedTrack(val track: Track = Track())
