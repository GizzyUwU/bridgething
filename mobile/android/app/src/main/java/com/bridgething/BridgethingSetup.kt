package com.bridgething

import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import com.margelo.nitro.bridgething.session.HybridBridgethingSession
import com.bridgething.applemusic.AppleMusicGlue
import com.bridgething.companion.HostInfo
import com.bridgething.lyrics.LrclibResolver
import com.bridgething.spotify.AndroidConnectivityWatcher
import com.bridgething.spotify.SpotifyGlue
import uniffi.spotify.TokenStore as SpTokenStore

public object BridgethingApp {
    public const val APP_NAME: String = "bridgething"
    public const val SPOTIFY_PROVIDER_ID: String = "spotify"

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
        )
        HybridBridgethingSessionImpl.lyricsResolver = LrclibResolver()

        val spotifyTokenStore = SpotifyKeychainStore(app)

        HybridBridgethingSessionImpl.registry = listOf(
            HybridBridgethingSessionImpl.ProviderRegistration(
                id = "spotify",
                displayName = "Spotify",
                available = true,
                factory = { makeSpotifyGlue(spotifyTokenStore, app.cacheDir, app) },
                signOut = { spotifyTokenStore.clear() },
                hasCredentials = { spotifyTokenStore.loadRefreshToken() != null },
            ),
            HybridBridgethingSessionImpl.ProviderRegistration(
                id = "appleMusic",
                displayName = "Apple Music",
                available = false,
                factory = { AppleMusicGlue() },
                signOut = {},
            ),
        )

        HybridBridgethingSession.installBackend(HybridBridgethingSessionImpl(app))
    }

    private fun makeSpotifyGlue(
        store: SpotifyKeychainStore,
        cacheDir: java.io.File,
        appContext: android.content.Context,
    ): SpotifyGlue =
        SpotifyGlue(
            workerBase = SPOTIFY_WORKER_BASE,
            psk = BuildConfig.BRIDGETHING_AUTH_PSK,
            deviceId = store.deviceId(),
            tokenStore = store,
            cacheDir = cacheDir,
            appContext = appContext,
            connectivity = AndroidConnectivityWatcher(appContext),
        )
}

private class SpotifyKeychainStore(context: Context) : SpTokenStore {
    private val prefs: SharedPreferences? by lazy {
        try {
            val masterKey = MasterKey.Builder(context)
                .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                .build()
            @Suppress("DEPRECATION")
            EncryptedSharedPreferences.create(
                context,
                "com.bridgething.spotify",
                masterKey,
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )
        } catch (e: Exception) {
            Log.e("BridgethingSetup", "spotify token store unavailable", e)
            null
        }
    }

    override fun loadRefreshToken(): String? = prefs?.getString("refresh", null)
    override fun saveRefreshToken(token: String) { prefs?.edit()?.putString("refresh", token)?.apply() }
    override fun loadUsername(): String? = prefs?.getString("username", null)
    override fun saveUsername(username: String) { prefs?.edit()?.putString("username", username)?.apply() }

    fun clear() {
        prefs?.edit()?.remove("refresh")?.remove("username")?.apply()
    }

    fun deviceId(): String {
        prefs?.getString("device_id", null)?.let { return it }
        val bytes = ByteArray(20)
        java.security.SecureRandom().nextBytes(bytes)
        val id = bytes.joinToString("") { "%02x".format(it) }
        prefs?.edit()?.putString("device_id", id)?.apply()
        return id
    }
}
