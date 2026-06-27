import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import uniffi.spotify.AuthState
import uniffi.spotify.Device
import uniffi.spotify.LibraryScope
import uniffi.spotify.Observer
import uniffi.spotify.PlayerState
import uniffi.spotify.Queue
import uniffi.spotify.SpotifyClient
import uniffi.spotify.TokenStore
import java.io.File

class FileStore(private val dir: String) : TokenStore {
    init { File(dir).mkdirs() }
    private fun read(name: String): String? =
        File(dir, name).takeIf { it.exists() }?.readText()?.trim()?.ifEmpty { null }
    private fun write(name: String, value: String) = File(dir, name).writeText(value)
    override fun loadRefreshToken(): String? = read(".refresh_token.txt")
    override fun saveRefreshToken(token: String) = write(".refresh_token.txt", token)
    override fun loadUsername(): String? = read(".username")
    override fun saveUsername(username: String) = write(".username", username)
}

class Printer : Observer {
    override fun onPlayer(state: PlayerState) {
        val track = state.track?.let { "${it.name} - ${it.artists.joinToString(", ") { a -> a.name }}" } ?: "(nothing)"
        println("[kotlin player] $track | ${if (state.isPaused) "paused" else "playing"} | ${state.positionMs / 1000u}s/${state.durationMs / 1000u}s")
    }
    override fun onQueue(queue: Queue) = println("[kotlin queue] ${queue.next.size} upcoming")
    override fun onDevices(devices: List<Device>) = println("[kotlin devices] ${devices.joinToString(", ") { it.name }}")
    override fun onAuth(state: AuthState) = println("[kotlin auth] $state")
    override fun onLibraryChanged(scope: LibraryScope) = println("[kotlin library] changed: $scope")
}

fun main() = runBlocking {
    val psk = System.getenv("SPOTIFY_AUTH_PSK") ?: error("need SPOTIFY_AUTH_PSK")
    val state = System.getenv("SPOTIFY_PRIVATE_STATE") ?: "/tmp/sfp-live"
    val deviceId = System.getenv("SPOTIFY_DEVICE_ID") ?: "00112233445566778899aabbccddeeff00112233"

    val store = FileStore(state)
    if (store.loadRefreshToken() == null) {
        System.getenv("SPOTIFY_CARTHING_REFRESH_TOKEN")?.takeIf { it.isNotEmpty() }?.let { store.saveRefreshToken(it) }
    }

    val client = SpotifyClient.create("https://thinglabs.sh/auth", psk, deviceId, store, Printer())
    try {
        client.connect()
        println("[kotlin] connected")
    } catch (e: Exception) {
        println("[kotlin] connect error: $e")
    }
    println("[kotlin] watching 12s...")
    delay(12_000)
    println("[kotlin] done")
}
