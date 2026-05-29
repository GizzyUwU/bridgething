package dev.bridgething.spotify

class UsersResource(private val client: SpotinyClient) {
    suspend fun getCurrentUser(): User? {
        val urlStr = "$SPOTIFY_API_BASE/v1/me?fields=id%2Curi%2Cdisplay_name%2Cproduct%2Cimages"
        return client.http.getJson<User>(urlStr)
    }
}
