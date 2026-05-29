package dev.bridgething.spotify

import kotlinx.serialization.Serializable

class EpisodesResource(private val client: SpotinyClient) {
    suspend fun getEpisode(uri: SpotifyUri?): Episode? {
        if (uri == null) return null

        val urlStr =
            "$SPOTIFY_API_BASE/v1/episodes/${uri.id}?fields=id%2Curi%2Cname%2Cdescription%2Cexplicit%2Cduration_ms%2Cimages%2Crelease_date%2Cshow%28id%2Curi%2Cname%2Cdescription%2Cpublisher%2Cexplicit%2Cimages%29"

        return client.http.getJson<Episode>(urlStr)
    }

    suspend fun getUserSavedEpisodes(limit: Int? = null, offset: Int? = null): ItemsPage<Episode> {
        val urlStr =
            "$SPOTIFY_API_BASE/v1/me/episodes?fields=next%2Ctotal%2Citems%28episode%28id%2Curi%2Cname%2Cdescription%2Cexplicit%2Cduration_ms%2Cimages%2Crelease_date%2Cshow%28id%2Curi%2Cname%2Cdescription%2Cpublisher%2Cexplicit%2Cimages%29%29%29"

        val page = client.http.getItems<SavedEpisode>(urlStr, limit = limit, offset = offset)

        return page.map { it.episode }
    }
}

@Serializable
private data class SavedEpisode(val episode: Episode = Episode())
