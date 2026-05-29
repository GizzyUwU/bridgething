package dev.bridgething.spotify

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

class RecommendationsResource(private val client: SpotinyClient) {
    suspend fun get(
        seedTracks: List<String> = emptyList(),
        seedArtists: List<String> = emptyList(),
        seedGenres: List<String> = emptyList(),
        limit: Int,
    ): List<Track> {
        if (seedTracks.isEmpty() && seedArtists.isEmpty() && seedGenres.isEmpty()) return emptyList()

        val clampedLimit = limit.coerceIn(1, 100)

        val query = mutableMapOf("limit" to clampedLimit.toString())
        if (seedTracks.isNotEmpty()) query["seed_tracks"] = seedTracks.take(5).joinToString(",")
        if (seedArtists.isNotEmpty()) query["seed_artists"] = seedArtists.take(5).joinToString(",")
        if (seedGenres.isNotEmpty()) query["seed_genres"] = seedGenres.take(5).joinToString(",")

        val url = client.http.buildUrl("$SPOTIFY_API_BASE/v1/recommendations", query)

        val response = client.http.getJson<JsonObject>(url) ?: return emptyList()
        val tracks = response["tracks"] as? JsonArray ?: return emptyList()
        return client.http.decodeLossyArray(tracks.toString(), Track.serializer()) ?: emptyList()
    }
}
