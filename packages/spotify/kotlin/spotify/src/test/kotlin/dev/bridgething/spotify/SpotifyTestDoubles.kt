package dev.bridgething.spotify

import dev.bridgething.gateway.Adapter
import dev.bridgething.gateway.AdapterEvent
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow

internal class NoOpAdapter : Adapter {
    override val events: Flow<AdapterEvent> = MutableSharedFlow()
    override suspend fun start() {}
    override suspend fun stop() {}
    override suspend fun disconnect(deviceId: String) {}
    override suspend fun send(deviceId: String, frame: ByteArray) {}
}

internal class StubAuthenticator : SpotifyAuthenticator {
    override suspend fun authorize(): TokenBundle = TokenBundle("t", "r", "Bearer", 3600, null)
    override suspend fun refreshAccessToken(refreshToken: String): TokenBundle =
        TokenBundle("t", "r", "Bearer", 3600, null)
}
