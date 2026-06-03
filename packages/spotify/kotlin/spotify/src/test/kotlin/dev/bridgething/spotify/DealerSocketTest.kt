package dev.bridgething.spotify

import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

private class FakeDealerProvider : DealerSocketProvider {
    var listener: DealerSocketListener? = null
    val sent = mutableListOf<String>()

    override suspend fun open(accessToken: String, listener: DealerSocketListener) {
        this.listener = listener
        listener.onOpen()
    }

    override suspend fun send(text: String) {
        sent.add(text)
    }

    override suspend fun close() {
        listener?.onClosed()
    }
}

private class RecordingDelegate : SpotinyDelegate {
    val states = mutableListOf<PlayerState>()
    var connected = false

    override fun authDidRefresh(accessToken: String, refreshToken: String) {}
    override fun authDidFail(reason: String) {}
    override fun playerStateUpdated(oldState: PlayerState?, newState: PlayerState) {
        states.add(newState)
    }
    override fun socketDidConnect() { connected = true }
    override fun socketDidDisconnect() { connected = false }
}

private const val SEED_STATE = """
    {"is_playing":true,"progress_ms":5000,"shuffle_state":false,"repeat_state":"off",
     "currently_playing_type":"track",
     "item":{"type":"track","id":"1","uri":"spotify:track:1","name":"SeedSong","duration_ms":200000,
       "artists":[{"id":"a","uri":"spotify:artist:a","name":"Artist","type":"artist"}]}}
"""

private const val EVENT_ENVELOPE = """
    {"headers":{"content-type":"application/json"},"uri":"wss://event","payloads":[{"events":[
      {"source":"player","type":"PLAYER_STATE_CHANGED","event":{"event_id":1,"state":
        {"is_playing":true,"progress_ms":0,"shuffle_state":false,"repeat_state":"off",
         "currently_playing_type":"track",
         "item":{"type":"track","id":"2","uri":"spotify:track:2","name":"EventSong","duration_ms":180000,
           "artists":[{"id":"a","uri":"spotify:artist:a","name":"Artist","type":"artist"}]}}}}
    ]}]}
"""

class DealerSocketTest {
    private fun client(delegate: SpotinyDelegate): SpotinyClient {
        val engine = MockEngine { request ->
            if (request.url.encodedPath.startsWith("/v1/me/player")) {
                respond(content = SEED_STATE, status = HttpStatusCode.OK)
            } else {
                respond(content = "", status = HttpStatusCode.OK)
            }
        }
        return SpotinyClient(StubAuthenticator(), delegate = delegate, accessToken = "token", refreshToken = "r", engine = engine)
    }

    @Test
    fun seedsStateOnOpenThenDispatchesPlayerStateEvents() = runBlocking {
        val delegate = RecordingDelegate()
        val provider = FakeDealerProvider()
        val dealer = DealerSocket(client(delegate), provider)

        dealer.start()
        // onOpen seeds now-playing from the rest endpoint.
        assertTrue(delegate.connected)
        assertEquals("SeedSong", delegate.states.last().item?.name)

        // a dealer player-state push parses inline and dispatches without a rest roundtrip.
        provider.listener!!.onText(EVENT_ENVELOPE)
        assertEquals("EventSong", delegate.states.last().item?.name)

        dealer.stop()
        assertTrue(!delegate.connected)
    }

    @Test
    fun heartbeatPongIsIgnored() = runBlocking {
        val delegate = RecordingDelegate()
        val provider = FakeDealerProvider()
        val dealer = DealerSocket(client(delegate), provider)

        dealer.start()
        val seeded = delegate.states.size
        provider.listener!!.onText("""{"type": "pong"}""")
        assertEquals(seeded, delegate.states.size)

        dealer.stop()
    }
}
