package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.lyrics.Lyrics
import dev.bridgething.lyrics.LyricsResolver
import dev.bridgething.lyrics.TrackIdentity

/** No-op geo backend so the companion boots without location services. */
object NoOpGeoSource : GeoSource {
    override suspend fun start(gateway: BridgethingGateway) {}
    override suspend fun stop() {}
}

/** No-op volume backend so the companion boots without a real AudioManager. */
object NoOpVolumeSource : VolumeSource {
    override fun start(callback: VolumeSource.Callback) {}
    override fun stop() {}
    override fun snapshot(): Pair<Float, Boolean> = 0f to false
}

/** Lyrics resolver that always falls through (no synced/plain lyrics). */
class FakeLyricsResolver : LyricsResolver {
    override val name: String = "fake"
    override suspend fun lyrics(track: TrackIdentity): Lyrics? = null
}
