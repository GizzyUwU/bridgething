package dev.bridgething.spotify

import kotlinx.serialization.Serializable

class ArtistsResource(private val client: SpotinyClient) {
    suspend fun getArtist(uri: SpotifyUri?): Artist? {
        if (uri == null) return null

        val urlStr = "$SPOTIFY_API_BASE/v1/artists/${uri.id}/?fields=id%2Curi%2Cname%2Ctype%2Cimages"

        return client.http.getJson<Artist>(urlStr)
    }

    suspend fun getArtistTopTracks(uri: SpotifyUri?): List<Track> {
        if (uri == null) return emptyList()

        val urlStr =
            "$SPOTIFY_API_BASE/v1/artists/${uri.id}/top-tracks?fields=tracks%28id%2Curi%2Cname%2Cexplicit%2Cduration_ms%2Calbum%28id%2Curi%2Cname%2Cimages%2Cartists%28id%2Curi%2Cname%2Ctype%29%29%2Cartists%28id%2Curi%2Cname%2Ctype%29%29"

        return client.http.getJson<TopTracks>(urlStr)?.tracks ?: emptyList()
    }

    suspend fun getUserFollowedArtists(limit: Int? = null, offset: Int? = null): ItemsPage<Artist> {
        val urlStr =
            "$SPOTIFY_API_BASE/v1/me/following?type=artist&fields=artists%28next%2Ctotal%2Citems%28id%2Curi%2Cname%2Ctype%2Cimages%29%29"

        return client.http.getFollowedArtists(urlStr, limit = limit, offset = offset)
    }
}

@Serializable
private data class TopTracks(
    @Serializable(with = LossyListSerializer::class) val tracks: List<Track> = emptyList(),
)
