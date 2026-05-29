package dev.bridgething.spotify

import io.ktor.client.engine.HttpClientEngine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

interface SpotinyDelegate {
    fun authDidRefresh(accessToken: String, refreshToken: String)
    fun authDidFail(reason: String)
    fun playerStateUpdated(oldState: PlayerState?, newState: PlayerState)
    fun socketDidConnect()
    fun socketDidDisconnect()

    // service health: the web api throttled us (429), and later recovered.
    fun serviceDidRateLimit(retryAfterSeconds: Int) {}
    fun serviceDidRecover() {}
}

/**
 * spotify web api client. authenticates via the injected authenticator and exposes typed resources.
 * token persistence is caller-owned through delegate.authDidRefresh. the dealer websocket is not started; connect() authenticates only.
 */
class SpotinyClient(
    private val authenticator: SpotifyAuthenticator,
    var delegate: SpotinyDelegate? = null,
    accessToken: String = "",
    refreshToken: String = "",
    engine: HttpClientEngine? = null,
) {
    val maxRetries = 3

    var accessToken: String = accessToken
        private set
    var refreshToken: String = refreshToken
        private set

    var needsReAuth = false
        private set
    var isConnected = false
        private set
    var authFailed = false
        private set
    var lastAuthError: String? = null
        private set

    val hasAuthTokens: Boolean
        get() = accessToken.isNotEmpty() && refreshToken.isNotEmpty()

    val http = SpotinyHttp(this, engine)

    val albums by lazy { AlbumsResource(this) }
    val artists by lazy { ArtistsResource(this) }
    val categories by lazy { CategoriesResource(this) }
    val episodes by lazy { EpisodesResource(this) }
    val library by lazy { LibraryResource(this) }
    val player by lazy { PlayerResource(this) }
    val playlists by lazy { PlaylistsResource(this) }
    val recommendations by lazy { RecommendationsResource(this) }
    val search by lazy { SearchResource(this) }
    val shows by lazy { ShowsResource(this) }
    val tracks by lazy { TracksResource(this) }
    val users by lazy { UsersResource(this) }

    private val authMutex = Mutex()

    suspend fun connect() {
        if (isConnected) return
        isConnected = false
        authenticate()
    }

    suspend fun reauthenticate() {
        if (authFailed || !needsReAuth) return
        authenticate()
    }

    fun setNeedsReAuth(value: Boolean) {
        needsReAuth = value
    }

    fun setTokens(accessToken: String, refreshToken: String) {
        this.accessToken = accessToken
        this.refreshToken = refreshToken
    }

    private suspend fun authenticate(): Boolean = authMutex.withLock {
        var refreshError: Throwable? = null

        val currentRefresh = refreshToken
        if (currentRefresh.isNotEmpty()) {
            try {
                val token = authenticator.refreshAccessToken(currentRefresh)
                accessToken = token.accessToken
                token.refreshToken?.let { refreshToken = it }
                notifyAuthDidRefresh()
                lastAuthError = null
                setAuthFailed(false)
                needsReAuth = false
                return@withLock true
            } catch (e: Throwable) {
                refreshError = e
                accessToken = ""
                refreshToken = ""
            }
        }

        try {
            val token = authenticator.authorize()
            accessToken = token.accessToken
            refreshToken = token.refreshToken ?: ""
            notifyAuthDidRefresh()
            lastAuthError = null
            setAuthFailed(false)
            needsReAuth = false
            true
        } catch (e: Throwable) {
            accessToken = ""
            refreshToken = ""
            notifyAuthDidRefresh()
            lastAuthError = describeAuthError(e, refreshError)
            setAuthFailed(true)
            false
        }
    }

    private fun notifyAuthDidRefresh() {
        delegate?.authDidRefresh(accessToken, refreshToken)
    }

    private fun setAuthFailed(value: Boolean) {
        authFailed = value
        if (value) delegate?.authDidFail(lastAuthError ?: "authentication failed")
    }
}

private fun describeAuthError(authorize: Throwable, refresh: Throwable?): String {
    val primary = describeOAuthError(authorize)
    if (refresh == null) return primary
    return "$primary (refresh first failed: ${describeOAuthError(refresh)})"
}

private fun describeOAuthError(error: Throwable): String = when (error) {
    is OAuthError.MissingAuthorizationCode -> "missing authorization code"
    is OAuthError.RandomGenerationFailed -> "random generation failed"
    is OAuthError.UnsupportedPlatform -> "OAuth flow not supported on this platform"
    is OAuthError.MalformedDeviceCodeResponse -> "device-code response was malformed"
    is OAuthError.TokenRequestFailed -> "token endpoint ${error.status}: ${error.body}"
    is OAuthError.AuthorizationFailed ->
        error.description?.let { "${error.error}: $it" } ?: error.error
    else -> error.message ?: error.toString()
}
