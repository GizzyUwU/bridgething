package com.bridgething

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import com.margelo.nitro.bridgething.session.BridgethingSpotifyAuthConfig
import com.margelo.nitro.bridgething.session.HybridBridgethingSession
import dev.bridgething.applemusic.AppleMusicGlue
import dev.bridgething.companion.HostInfo
import dev.bridgething.lyrics.LrclibResolver
import dev.bridgething.spotify.DeviceCodeAuthenticator
import dev.bridgething.spotify.DeviceCodeConfig
import dev.bridgething.spotify.PkceRefreshAuthenticator
import dev.bridgething.spotify.PkceRefreshConfig
import dev.bridgething.spotify.SpotifyAuthenticatorFactory
import dev.bridgething.spotify.SpotifyGlue
import dev.bridgething.tidal.TidalGlue

/**
 * Wires the Nitro session module's static registry and installs the real
 * session backend before React Native starts. Each provider registration
 * carries a factory closure that produces a fresh glue plus a `signOut`
 * hook that clears persisted credentials.
 */
public object BridgethingApp {
    public const val APP_NAME: String = "bridgething"

    private const val AUTH_WORKER_BASE_URL: String = "https://thinglabs.sh/auth"

    public const val SPOTIFY_PROVIDER_ID: String = "spotify"

    private val SPOTIFY_SCOPES: List<String> = listOf(
        "user-read-playback-state",
        "user-modify-playback-state",
        "user-read-currently-playing",
        "user-read-playback-position",
        "user-top-read",
        "user-read-recently-played",
        "playlist-read-private",
        "playlist-read-collaborative",
        "playlist-modify-private",
        "playlist-modify-public",
        "user-follow-modify",
        "user-follow-read",
        "user-library-read",
        "user-library-modify",
        "user-read-private",
    )

    public fun spotifyAuthConfig(): BridgethingSpotifyAuthConfig = BridgethingSpotifyAuthConfig(
        scopes = SPOTIFY_SCOPES.toTypedArray(),
        pkceClientId = BuildConfig.BRIDGETHING_PKCE_CLIENT_ID,
        pkceRedirectUri = "https://discord.com/api/connections/spotify/callback",
        pkceAuthorizeUrl = "https://accounts.spotify.com/authorize",
        pkceTokenUrl = "https://accounts.spotify.com/api/token",
        deviceCodePsk = BuildConfig.BRIDGETHING_AUTH_PSK,
        deviceCodeUrl = "$AUTH_WORKER_BASE_URL/api/device/code",
        deviceCodeTokenUrl = "$AUTH_WORKER_BASE_URL/api/token",
        deviceCodeDescription = "car-thing-device",
    )

    public fun persistSpotifyTokens(context: Context, access: String, refresh: String, usesDealer: Boolean) {
        SpotifyTokenStore(context.applicationContext)
            .save(SpotifyTokenStore.Tokens(access.ifEmpty { null }, refresh.ifEmpty { null }, usesDealer))
    }

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

        val spotifyTokenStore = SpotifyTokenStore(app)

        HybridBridgethingSessionImpl.registry = listOf(
            HybridBridgethingSessionImpl.ProviderRegistration(
                id = "spotify",
                displayName = "Spotify",
                available = true,
                factory = { makeSpotifyGlue(spotifyTokenStore) },
                signOut = { spotifyTokenStore.clear() },
                hasCredentials = { !spotifyTokenStore.load().refresh.isNullOrEmpty() },
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

        HybridBridgethingSession.installBackend(HybridBridgethingSessionImpl(app))
    }

    private fun makeSpotifyGlue(store: SpotifyTokenStore): SpotifyGlue {
        val seed = store.load()
        val usesDealer = seed.usesDealer
        val authenticatorFactory: SpotifyAuthenticatorFactory
        if (usesDealer) {
            val pkceConfig = PkceRefreshConfig(
                clientId = BuildConfig.BRIDGETHING_PKCE_CLIENT_ID,
                tokenUrl = "https://accounts.spotify.com/api/token",
            )
            authenticatorFactory = { PkceRefreshAuthenticator(pkceConfig) }
        } else {
            val deviceConfig = DeviceCodeConfig(
                workerBaseUrl = AUTH_WORKER_BASE_URL,
                authorizationBearer = BuildConfig.BRIDGETHING_AUTH_PSK,
                scopes = SPOTIFY_SCOPES,
                description = "car-thing-device",
            )
            authenticatorFactory = { DeviceCodeAuthenticator(deviceConfig) }
        }
        return SpotifyGlue(
            authenticatorFactory = authenticatorFactory,
            accessToken = seed.access ?: "",
            refreshToken = seed.refresh ?: "",
            onTokensRefreshed = { access, refresh ->
                store.save(SpotifyTokenStore.Tokens(access.ifEmpty { null }, refresh.ifEmpty { null }, usesDealer))
            },
            usesDealer = usesDealer,
        )
    }
}

private class SpotifyTokenStore(private val context: Context) {
    data class Tokens(val access: String?, val refresh: String?, val usesDealer: Boolean = false)

    private val prefs: SharedPreferences by lazy {
        val masterKey = MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        @Suppress("DEPRECATION")
        EncryptedSharedPreferences.create(
            context,
            "dev.bridgething.spotify",
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    fun load(): Tokens = Tokens(
        prefs.getString("access", null),
        prefs.getString("refresh", null),
        prefs.getBoolean("usesDealer", false),
    )

    fun save(tokens: Tokens) {
        prefs.edit()
            .apply {
                if (tokens.access != null) putString("access", tokens.access) else remove("access")
                if (tokens.refresh != null) putString("refresh", tokens.refresh) else remove("refresh")
                putBoolean("usesDealer", tokens.usesDealer)
            }
            .apply()
    }

    fun clear() {
        prefs.edit().remove("access").remove("refresh").remove("usesDealer").apply()
    }
}
