package com.bridgething

import android.content.Context
import com.margelo.nitro.bridgething.session.HybridBridgethingSession
import uniffi.bridgething_companion.HostInfo
import uniffi.bridgething_companion.SpotifyProviderConfig

public object BridgethingApp {
    public const val APP_NAME: String = "bridgething"

    private const val SPOTIFY_WORKER_BASE: String = "https://thinglabs.sh/auth"

    public fun installBridgething(context: Context) {
        val app = context.applicationContext
        val pkg = try {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "0.0.0"
        } catch (_: android.content.pm.PackageManager.NameNotFoundException) { "0.0.0" }

        HybridBridgethingSessionImpl.hostInfo = HostInfo(
            appName = APP_NAME,
            appVersion = pkg,
            osName = "Android",
            osVersion = "",
            hostIdentifier = "",
        )
        HybridBridgethingSessionImpl.spotifyConfig = SpotifyProviderConfig(
            workerBase = SPOTIFY_WORKER_BASE,
            psk = BuildConfig.BRIDGETHING_AUTH_PSK,
        )

        HybridBridgethingSession.installBackend(HybridBridgethingSessionImpl(app))
    }
}
