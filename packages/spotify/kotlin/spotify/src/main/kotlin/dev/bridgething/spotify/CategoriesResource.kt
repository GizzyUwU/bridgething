package dev.bridgething.spotify

class CategoriesResource(private val client: SpotinyClient) {
    suspend fun getMadeForYou(limit: Int? = null, offset: Int? = null): ItemsPage<Playlist> {
        val url = "https://api.spotify.com/v1/browse/categories/0JQ5DAt0tbjZptfcdMSKl3/playlists"

        return client.http.getCategoryPlaylists(url, limit = limit, offset = offset)
            .filter { playlist ->
                !(playlist.id == "37i9dQZF1EYkqdzj48dyYq") && !playlist.name.lowercase().endsWith(" you")
            }
    }
}
