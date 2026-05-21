package com.bridgething

import android.content.Context
import com.margelo.nitro.bridgething.session.HybridBridgethingSession
import dev.bridgething.applemusic.AppleMusicGlue
import dev.bridgething.companion.HostInfo
import dev.bridgething.lyrics.LrclibResolver
import dev.bridgething.spotify.SpotifyGlue
import dev.bridgething.tidal.TidalGlue

/**
 * Wires the bridgething Nitro session module's static registry and
 * installs the real session backend before React Native starts.
 * [MainApplication.onCreate] calls [installBridgething] after the
 * default react host is built.
 *
 * Mirror of the iOS `BridgethingApp` setup. Provider registrations land
 * in `HybridBridgethingSessionImpl.registry`; each carries a factory
 * closure that produces a fresh glue plus a `signOut` hook that clears
 * the host's persisted credentials.
 */
public object BridgethingApp {
    public const val APP_NAME: String = "bridgething"

    public fun installBridgething(context: Context) {
        val pkg = try {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "0.0.0"
        } catch (_: android.content.pm.PackageManager.NameNotFoundException) { "0.0.0" }

        HybridBridgethingSessionImpl.hostInfo = HostInfo(
            appName = APP_NAME,
            appVersion = pkg,
            osName = "Android",
        )
        HybridBridgethingSessionImpl.lyricsResolver = LrclibResolver()

        // Spotify auth on Android lands in a follow-up; `SpotifyGlue.attach`
        // throws `NotImplemented` for now so the registration is "available
        // = false" until the real auth + dealer client is wired up.
        HybridBridgethingSessionImpl.registry = listOf(
            HybridBridgethingSessionImpl.ProviderRegistration(
                id = "spotify",
                displayName = "Spotify",
                available = false,
                factory = { stubSpotifyGlue() },
                signOut = {},
            ),
            HybridBridgethingSessionImpl.ProviderRegistration(
                id = "appleMusic",
                displayName = "Apple Music",
                available = false,
                factory = { AppleMusicGlue() },
                signOut = {},
            ),
            HybridBridgethingSessionImpl.ProviderRegistration(
                id = "tidal",
                displayName = "Tidal",
                available = false,
                factory = { TidalGlue() },
                signOut = {},
            ),
        )

        HybridBridgethingSession.installBackend(HybridBridgethingSessionImpl(context.applicationContext))
    }

    private fun stubSpotifyGlue(): SpotifyGlue = SpotifyGlue(
        authenticator = object : dev.bridgething.spotify.SpotifyAuthenticator {
            override suspend fun authorize(): dev.bridgething.spotify.TokenBundle =
                throw NotImplementedError("Spotify Android auth lands in a follow-up slice")
            override suspend fun refreshAccessToken(refreshToken: String): dev.bridgething.spotify.TokenBundle =
                throw NotImplementedError("Spotify Android auth lands in a follow-up slice")
        },
    )
}
