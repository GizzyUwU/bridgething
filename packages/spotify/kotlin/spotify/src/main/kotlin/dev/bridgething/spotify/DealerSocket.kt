package dev.bridgething.spotify

import io.ktor.client.HttpClient
import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.websocket.DefaultClientWebSocketSession
import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.websocket.Frame
import io.ktor.websocket.close
import io.ktor.websocket.readText
import io.ktor.websocket.send
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

const val DEALER_PREFIX = "wss://dealer.spotify.com/?access_token="
const val SUBSCRIPTION_PREFIX = "https://api.spotify.com/v1/me/notifications/player?connection_id="

private const val HEARTBEAT_INTERVAL_MS = 30_000L
private const val PLAYER_STATE_CHANGED = "PLAYER_STATE_CHANGED"

interface DealerSocketListener {
    suspend fun onOpen()
    suspend fun onText(text: String)
    suspend fun onClosed()
}

interface DealerSocketProvider {
    suspend fun open(accessToken: String, listener: DealerSocketListener)
    suspend fun send(text: String)
    suspend fun close()
}

class KtorDealerSocketProvider(engine: HttpClientEngine? = null) : DealerSocketProvider {
    private val http: HttpClient =
        engine?.let { HttpClient(it) { install(WebSockets) } } ?: HttpClient(CIO) { install(WebSockets) }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var session: DefaultClientWebSocketSession? = null
    private var receiveJob: Job? = null

    override suspend fun open(accessToken: String, listener: DealerSocketListener) {
        val s = http.webSocketSession(DEALER_PREFIX + accessToken)
        session = s
        listener.onOpen()
        receiveJob = scope.launch {
            try {
                for (frame in s.incoming) {
                    if (frame is Frame.Text) listener.onText(frame.readText())
                }
            } catch (_: Throwable) {
            } finally {
                listener.onClosed()
            }
        }
    }

    override suspend fun send(text: String) {
        session?.send(Frame.Text(text))
    }

    override suspend fun close() {
        runCatching { session?.close() }
        session = null
    }
}

class DealerSocket(
    private val client: SpotinyClient,
    private val provider: DealerSocketProvider,
) : DealerSocketListener {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val json = Json { ignoreUnknownKeys = true; isLenient = true; coerceInputValues = true }
    private var heartbeatJob: Job? = null

    @Volatile private var active = false

    suspend fun start() {
        active = true
        connect()
    }

    suspend fun stop() {
        active = false
        stopHeartbeat()
        provider.close()
        client.setConnected(false)
        scope.cancel()
    }

    private suspend fun connect() {
        var retries = 0
        while (active) {
            if (client.accessToken.isEmpty()) return
            try {
                provider.open(client.accessToken, this)
                return
            } catch (_: Throwable) {
                if (retries >= client.maxRetries) {
                    client.reauthenticate()
                    return
                }
                val retryIn = retries * 2 + 2
                retries += 1
                delay(retryIn * 1000L)
            }
            if (client.needsReAuth) return
        }
    }

    override suspend fun onOpen() {
        if (!active) return
        client.setConnected(true)
        client.delegate?.socketDidConnect()
        startHeartbeat()
        client.player.getPlaybackState()?.let { dispatchState(it) }
    }

    override suspend fun onText(text: String) {
        if (!active) return
        if (isHeartbeatPong(text)) return

        val envelope = runCatching { json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
        val headers = envelope["headers"] as? JsonObject
        val connectionId = headers?.entries
            ?.firstOrNull { it.key.equals("Spotify-Connection-Id", ignoreCase = true) }
            ?.value?.jsonPrimitive?.contentOrNull
        if (connectionId != null) {
            client.http.put(SUBSCRIPTION_PREFIX + connectionId)
            return
        }

        if (envelope["uri"]?.jsonPrimitive?.contentOrNull != "wss://event") return
        for (state in extractPlayerStates(envelope)) {
            handlePlayerState(state)
        }
    }

    override suspend fun onClosed() {
        client.setConnected(false)
        client.delegate?.socketDidDisconnect()
        stopHeartbeat()
        if (!active || client.needsReAuth) return
        connect()
    }

    private suspend fun handlePlayerState(state: PlayerState) {
        // episodes don't carry full state over the socket, so refetch via the rest endpoint.
        val resolved = if (state.currentlyPlayingType == "episode") {
            client.player.getPlaybackState() ?: return
        } else {
            state
        }
        dispatchState(resolved)
    }

    private fun dispatchState(state: PlayerState) {
        val old = client.lastPlayerState
        client.setLastPlayerState(state)
        client.delegate?.playerStateUpdated(old, state)
    }

    private fun extractPlayerStates(envelope: JsonObject): List<PlayerState> {
        val payloads = envelope["payloads"] as? JsonArray ?: return emptyList()
        val out = mutableListOf<PlayerState>()
        for (payload in payloads) {
            val events = (payload as? JsonObject)?.get("events") as? JsonArray ?: continue
            for (event in events) {
                val obj = event as? JsonObject ?: continue
                if (obj["type"]?.jsonPrimitive?.contentOrNull != PLAYER_STATE_CHANGED) continue
                val stateElement = (obj["event"] as? JsonObject)?.get("state") ?: continue
                val state = runCatching {
                    json.decodeFromJsonElement(PlayerState.serializer(), stateElement)
                }.getOrNull() ?: continue
                out.add(state)
            }
        }
        return out
    }

    private fun startHeartbeat() {
        if (heartbeatJob != null) return
        heartbeatJob = scope.launch {
            while (isActive) {
                delay(HEARTBEAT_INTERVAL_MS)
                if (!active) return@launch
                runCatching { provider.send("{\"type\": \"ping\"}") }
            }
        }
    }

    private fun stopHeartbeat() {
        heartbeatJob?.cancel()
        heartbeatJob = null
    }

    private fun isHeartbeatPong(text: String): Boolean =
        text.contains("\"type\"") && text.contains("\"pong\"")
}
