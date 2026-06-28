package com.bridgething.lyrics

/**
 * Pluggable lyrics fetcher. Different impls hit different sources;
 * the consumer decides the fallback chain.
 */
interface LyricsResolver {
    val name: String
    suspend fun lyrics(track: TrackIdentity): Lyrics?
}

/** Identifies a track for lyrics lookup. Resolvers use what they need and ignore the rest. */
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
