package dev.bridgething.spotify

class SearchResource(private val client: SpotinyClient) {
    suspend fun search(query: String, types: List<String>, limit: Int, offset: Int): SpotifySearchResults {
        val trimmed = query.trim()
        if (trimmed.isEmpty() || types.isEmpty()) return SpotifySearchResults()

        val clampedLimit = limit.coerceIn(1, 50)
        val clampedOffset = maxOf(offset, 0)

        val url = client.http.buildUrl(
            "$SPOTIFY_API_BASE/v1/search",
            mapOf(
                "q" to trimmed,
                "type" to types.joinToString(","),
                "limit" to clampedLimit.toString(),
                "offset" to clampedOffset.toString(),
            ),
        )

        val response = client.http.getJson<SearchResponse>(url) ?: return SpotifySearchResults()

        return SpotifySearchResults(
            tracks = response.tracks?.items ?: emptyList(),
            albums = response.albums?.items ?: emptyList(),
            artists = response.artists?.items ?: emptyList(),
            playlists = response.playlists?.items ?: emptyList(),
            shows = response.shows?.items ?: emptyList(),
            episodes = response.episodes?.items ?: emptyList(),
        )
    }
}
