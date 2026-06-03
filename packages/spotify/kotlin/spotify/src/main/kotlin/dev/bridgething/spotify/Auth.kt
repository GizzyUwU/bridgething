package dev.bridgething.spotify

import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import kotlinx.coroutines.delay
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.net.URLEncoder

interface SpotifyAuthenticator {
    suspend fun authorize(): TokenBundle
    suspend fun refreshAccessToken(refreshToken: String): TokenBundle
}

data class TokenBundle(
    val accessToken: String,
    val refreshToken: String?,
    val tokenType: String?,
    val expiresIn: Int?,
    val scope: String?,
)

data class DeviceCodePrompt(
    val userCode: String,
    val verificationUrl: String,
    val verificationUrlComplete: String?,
)

sealed class OAuthError(message: String) : Exception(message) {
    object MissingAuthorizationCode : OAuthError("missing authorization code")
    object RandomGenerationFailed : OAuthError("random generation failed")
    object UnsupportedPlatform : OAuthError("unsupported platform")
    object MalformedDeviceCodeResponse : OAuthError("device-code response was malformed")

    data class TokenRequestFailed(val status: Int, val body: String) :
        OAuthError("token request failed (status=$status): $body")

    data class AuthorizationFailed(val error: String, val description: String?) :
        OAuthError("authorization failed: $error" + (description?.let { " ($it)" } ?: ""))
}

data class DeviceCodeConfig(
    val workerBaseUrl: String,
    val authorizationBearer: String,
    val scopes: List<String>,
    val clientId: String? = null,
    val description: String? = null,
)

class DeviceCodeAuthenticator(
    private val config: DeviceCodeConfig,
    private val onPrompt: (DeviceCodePrompt) -> Unit = {},
) : SpotifyAuthenticator {

    private val deviceCodeEndpoint: String = "${config.workerBaseUrl.trimEnd('/')}/api/device/code"
    private val tokenEndpoint: String = "${config.workerBaseUrl.trimEnd('/')}/api/token"

    private val authHeader: String = "Bearer ${config.authorizationBearer}"

    override suspend fun authorize(): TokenBundle {
        val prompt = requestDeviceCode()
        onPrompt(
            DeviceCodePrompt(
                userCode = prompt.userCode,
                verificationUrl = prompt.verificationUrl,
                verificationUrlComplete = prompt.verificationUrlComplete,
            )
        )
        return pollForTokens(
            deviceCode = prompt.deviceCode,
            interval = prompt.interval,
            expiresIn = prompt.expiresIn,
        )
    }

    override suspend fun refreshAccessToken(refreshToken: String): TokenBundle {
        val params = buildMap {
            put("grant_type", "refresh_token")
            put("refresh_token", refreshToken)
            config.clientId?.let { put("client_id", it) }
        }
        return tokenRequest(params)
    }

    private suspend fun requestDeviceCode(): DeviceCodeFlow {
        val params = buildMap {
            put("scope", config.scopes.joinToString(","))
            config.clientId?.let { put("client_id", it) }
            config.description?.let { put("description", it) }
        }

        val response = postForm(deviceCodeEndpoint, params)
        val status = response.status.value
        val text = response.bodyAsText()
        if (status !in 200..299) {
            throw OAuthError.TokenRequestFailed(status, text)
        }

        val body = runCatching { oauthJson.decodeFromString(DeviceCodeResponse.serializer(), text) }
            .getOrElse { throw OAuthError.MalformedDeviceCodeResponse }

        val userCode = body.userCode
        val deviceCode = body.deviceCode
        val verificationUrl = body.verificationUrl
        val verificationUrlPrefilled = body.verificationUrlPrefilled
        val expiresIn = body.expiresIn
        val interval = body.interval
        if (userCode == null || deviceCode == null || verificationUrl == null ||
            verificationUrlPrefilled == null || expiresIn == null || interval == null
        ) {
            throw OAuthError.MalformedDeviceCodeResponse
        }

        return DeviceCodeFlow(
            userCode = userCode,
            verificationUrl = verificationUrl,
            verificationUrlComplete = verificationUrlPrefilled,
            deviceCode = deviceCode,
            expiresIn = expiresIn,
            interval = interval,
        )
    }

    private suspend fun pollForTokens(deviceCode: String, interval: Int, expiresIn: Int): TokenBundle {
        val deadline = System.currentTimeMillis() + expiresIn * 1000L
        var currentInterval = maxOf(interval, 1)

        while (System.currentTimeMillis() < deadline) {
            delay(currentInterval * 1000L)

            val params = buildMap {
                put("grant_type", "urn:ietf:params:oauth:grant-type:device_code")
                put("device_code", deviceCode)
                config.clientId?.let { put("client_id", it) }
            }

            try {
                return tokenRequest(params)
            } catch (e: OAuthError.TokenRequestFailed) {
                when (errorString(e.body)) {
                    "authorization_pending" -> continue
                    "slow_down" -> {
                        currentInterval += 5
                        continue
                    }
                    "expired_token", "access_denied" ->
                        throw OAuthError.AuthorizationFailed(errorString(e.body) ?: "unknown", null)
                    else -> throw OAuthError.TokenRequestFailed(0, e.body)
                }
            }
        }

        throw OAuthError.AuthorizationFailed("expired_token", "local deadline reached")
    }

    private suspend fun tokenRequest(params: Map<String, String>): TokenBundle {
        val response = postForm(tokenEndpoint, params)
        val status = response.status.value
        val text = response.bodyAsText()
        if (status !in 200..299) {
            throw OAuthError.TokenRequestFailed(status, text)
        }
        val token = oauthJson.decodeFromString(OAuthTokenResponse.serializer(), text)
        return TokenBundle(
            accessToken = token.accessToken,
            refreshToken = token.refreshToken,
            tokenType = token.tokenType,
            expiresIn = token.expiresIn,
            scope = token.scope,
        )
    }

    private suspend fun postForm(endpoint: String, params: Map<String, String>): HttpResponse =
        oauthHttpClient.post(endpoint) {
            header("Authorization", authHeader)
            contentType(ContentType.Application.FormUrlEncoded)
            setBody(formUrlEncodedBody(params))
        }

    private fun errorString(body: String): String? =
        runCatching { oauthJson.decodeFromString(OAuthErrorBody.serializer(), body).error }.getOrNull()

    private data class DeviceCodeFlow(
        val userCode: String,
        val verificationUrl: String,
        val verificationUrlComplete: String,
        val deviceCode: String,
        val expiresIn: Int,
        val interval: Int,
    )
}

data class PkceRefreshConfig(
    val clientId: String,
    val tokenUrl: String,
)

class PkceRefreshAuthenticator(private val config: PkceRefreshConfig) : SpotifyAuthenticator {
    override suspend fun authorize(): TokenBundle = throw OAuthError.UnsupportedPlatform

    override suspend fun refreshAccessToken(refreshToken: String): TokenBundle {
        val params = mapOf(
            "grant_type" to "refresh_token",
            "refresh_token" to refreshToken,
            "client_id" to config.clientId,
        )
        val response = oauthHttpClient.post(config.tokenUrl) {
            contentType(ContentType.Application.FormUrlEncoded)
            setBody(formUrlEncodedBody(params))
        }
        val status = response.status.value
        val text = response.bodyAsText()
        if (status !in 200..299) throw OAuthError.TokenRequestFailed(status, text)
        val token = oauthJson.decodeFromString(OAuthTokenResponse.serializer(), text)
        return TokenBundle(
            accessToken = token.accessToken,
            refreshToken = token.refreshToken,
            tokenType = token.tokenType,
            expiresIn = token.expiresIn,
            scope = token.scope,
        )
    }
}

private val oauthHttpClient = HttpClient(CIO)

private val oauthJson = Json { ignoreUnknownKeys = true }

private fun formUrlEncodedBody(params: Map<String, String>): String =
    params.entries.joinToString("&") { (k, v) -> "${formEncode(k)}=${formEncode(v)}" }

private fun formEncode(value: String): String =
    URLEncoder.encode(value, "UTF-8")
        .replace("+", "%20")
        .replace("*", "%2A")
        .replace("%7E", "~")

@Serializable
private data class OAuthTokenResponse(
    @SerialName("access_token") val accessToken: String,
    @SerialName("refresh_token") val refreshToken: String? = null,
    @SerialName("token_type") val tokenType: String? = null,
    @SerialName("expires_in") val expiresIn: Int? = null,
    val scope: String? = null,
)

@Serializable
private data class DeviceCodeResponse(
    @SerialName("user_code") val userCode: String? = null,
    @SerialName("device_code") val deviceCode: String? = null,
    @SerialName("verification_url") val verificationUrl: String? = null,
    @SerialName("verification_url_prefilled") val verificationUrlPrefilled: String? = null,
    @SerialName("expires_in") val expiresIn: Int? = null,
    val interval: Int? = null,
)

@Serializable
private data class OAuthErrorBody(val error: String? = null)
