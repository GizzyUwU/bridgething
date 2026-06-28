package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.lyrics.Lyrics
import com.bridgething.lyrics.LyricsResolver
import com.bridgething.lyrics.TrackIdentity
import java.util.UUID

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

/** No-op audio backend so the companion boots without TextToSpeech / AudioManager. */
object NoOpAudioBackend : AudioBackend {
    override suspend fun setVolume(level: Float) {}
    override suspend fun setMute(muted: Boolean) {}
    override suspend fun volumeUp() {}
    override suspend fun volumeDown() {}
    override suspend fun muteToggle() {}
    override suspend fun speak(id: UUID, text: String, voice: String?, onStart: () -> Unit): Boolean {
        onStart()
        return true
    }
    override suspend fun cancel(id: UUID) {}
    override suspend fun cancelAll() {}
    override suspend fun playEarcon(name: String): Boolean = false
}

/** Lyrics resolver that returns [canned] (null by default = falls through). */
class FakeLyricsResolver(private val canned: Lyrics? = null) : LyricsResolver {
    override val name: String = "fake"
    override suspend fun lyrics(track: TrackIdentity): Lyrics? = canned
}
