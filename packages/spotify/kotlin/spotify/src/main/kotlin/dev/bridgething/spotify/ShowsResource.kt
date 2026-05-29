package dev.bridgething.spotify

import kotlinx.serialization.Serializable

class ShowsResource(private val client: SpotinyClient) {
    suspend fun getShow(uri: SpotifyUri?): Show? {
        if (uri == null) return null

        val urlStr =
            "https://api.spotify.com/v1/shows/${uri.id}?fields=id%2Curi%2Cname%2Cdescription%2Cpublisher%2Cexplicit%2Cimages"

        return client.http.getJson<Show>(urlStr)
    }

    suspend fun getShowEpisodes(uri: SpotifyUri?, limit: Int? = null, offset: Int? = null): ItemsPage<Episode> {
        if (uri == null) return ItemsPage.empty()

        val urlStr =
            "https://api.spotify.com/v1/shows/${uri.id}/episodes?fields=next%2Ctotal%2Citems%28id%2Curi%2Cname%2Cdescription%2Cexplicit%2Cduration_ms%2Cimages%2Crelease_date%2Cshow%28id%2Curi%2Cname%2Cdescription%2Cpublisher%2Cexplicit%2Cimages%29%29"

        return client.http.getItems<Episode>(urlStr, limit = limit, offset = offset)
    }

    suspend fun getUserSavedShows(limit: Int? = null, offset: Int? = null): ItemsPage<Show> {
        val urlStr =
            "https://api.spotify.com/v1/me/shows?fields=next%2Ctotal%2Citems%28show%28id%2Curi%2Cname%2Cdescription%2Cpublisher%2Cexplicit%2Cimages%29%29"

        val page: ItemsPage<SavedShow> = client.http.getItems<SavedShow>(urlStr, limit = limit, offset = offset)

        return page.map { it.show }
    }
}

@Serializable
private data class SavedShow(val show: Show = Show())
