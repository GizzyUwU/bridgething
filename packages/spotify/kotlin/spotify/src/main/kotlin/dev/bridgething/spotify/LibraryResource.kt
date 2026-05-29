package dev.bridgething.spotify

class LibraryResource(private val client: SpotinyClient) {
    // index-aligned with uris; unparseable or unsupported entries report false.
    suspend fun contains(uris: List<String>): List<Boolean> {
        val out = MutableList(uris.size) { false }

        val byKind = mutableMapOf<SpotifyUri.Kind, MutableList<Pair<Int, String>>>()
        for ((i, raw) in uris.withIndex()) {
            val uri = SpotifyUri.parse(raw) ?: continue
            byKind.getOrPut(uri.kind) { mutableListOf() }.add(i to uri.id)
        }

        for ((kind, entries) in byKind) {
            val url = containsUrl(kind, entries.map { it.second }) ?: continue
            val bools = client.http.getJson<List<Boolean>>(url) ?: continue
            for ((n, entry) in entries.withIndex()) {
                if (n < bools.size) out[entry.first] = bools[n]
            }
        }

        return out
    }

    suspend fun save(uris: List<SpotifyUri>) {
        mutate(uris, save = true)
    }

    suspend fun remove(uris: List<SpotifyUri>) {
        mutate(uris, save = false)
    }

    private suspend fun mutate(uris: List<SpotifyUri>, save: Boolean) {
        val idsByKind = mutableMapOf<SpotifyUri.Kind, MutableList<String>>()
        for (uri in uris) {
            idsByKind.getOrPut(uri.kind) { mutableListOf() }.add(uri.id)
        }

        for ((kind, ids) in idsByKind) {
            val url = mutateUrl(kind, ids) ?: continue
            if (save) client.http.put(url) else client.http.delete(url)
        }
    }

    private fun containsUrl(kind: SpotifyUri.Kind, ids: List<String>): String? {
        val csv = ids.joinToString(",")
        return when (kind) {
            SpotifyUri.Kind.TRACK -> "$SPOTIFY_API_BASE/v1/me/tracks/contains?ids=$csv"
            SpotifyUri.Kind.ALBUM -> "$SPOTIFY_API_BASE/v1/me/albums/contains?ids=$csv"
            SpotifyUri.Kind.SHOW -> "$SPOTIFY_API_BASE/v1/me/shows/contains?ids=$csv"
            SpotifyUri.Kind.EPISODE -> "$SPOTIFY_API_BASE/v1/me/episodes/contains?ids=$csv"
            SpotifyUri.Kind.ARTIST -> "$SPOTIFY_API_BASE/v1/me/following/contains?type=artist&ids=$csv"
            else -> null
        }
    }

    private fun mutateUrl(kind: SpotifyUri.Kind, ids: List<String>): String? {
        val csv = ids.joinToString(",")
        return when (kind) {
            SpotifyUri.Kind.TRACK -> "$SPOTIFY_API_BASE/v1/me/tracks?ids=$csv"
            SpotifyUri.Kind.ALBUM -> "$SPOTIFY_API_BASE/v1/me/albums?ids=$csv"
            SpotifyUri.Kind.SHOW -> "$SPOTIFY_API_BASE/v1/me/shows?ids=$csv"
            SpotifyUri.Kind.EPISODE -> "$SPOTIFY_API_BASE/v1/me/episodes?ids=$csv"
            SpotifyUri.Kind.ARTIST -> "$SPOTIFY_API_BASE/v1/me/following?type=artist&ids=$csv"
            else -> null
        }
    }
}
