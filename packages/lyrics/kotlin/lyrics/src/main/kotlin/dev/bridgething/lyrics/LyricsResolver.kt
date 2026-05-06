package dev.bridgething.lyrics

/**
 * Pluggable lyrics fetcher. Different impls hit different sources;
 * the consumer decides the fallback chain.
 */
interface LyricsResolver {
    val name: String
    suspend fun lyrics(track: TrackIdentity): Lyrics?
}

/**
 * Identifies a track for lyrics lookup. lrclib looks up by signature
 * (artist + track + album + duration); future resolvers may use ISRC
 * or platform-specific ids. Resolvers ignore what they don't need.
 */
data class TrackIdentity(
    val artist: String,
    val track: String,
    val album: String? = null,
    val durationMs: Int? = null,
    val isrc: String? = null,
)

data class Lyrics(
    val synced: List<LyricLine>?,
    val plain: String?,
    val source: String,
)

data class LyricLine(
    val startMs: Int,
    val text: String,
)
