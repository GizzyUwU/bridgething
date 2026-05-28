package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway

/**
 * Geo backend seam. [GeoController] is the real (FusedLocation / LocationManager)
 * impl; tests inject a no-op so the companion boots without touching location
 * services. Mirrors the Swift companion's geo DI seam.
 */
public interface GeoSource {
    public suspend fun start(gateway: BridgethingGateway)
    public suspend fun stop()
}

/**
 * Volume backend seam. [VolumeMonitor] is the real [android.media.AudioManager]
 * impl (which touches Android at construction); tests inject a no-op so the
 * companion can run on a plain JVM without Robolectric.
 */
public interface VolumeSource {
    public fun interface Callback {
        public fun onVolumeChanged(level: Float, muted: Boolean)
    }

    public fun start(callback: Callback)
    public fun stop()
    public fun snapshot(): Pair<Float, Boolean>
}
