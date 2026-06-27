package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.gateway.authority
import dev.bridgething.gateway.player
import dev.bridgething.glue.NowPlayingSink
import dev.bridgething.glue.NowPlayingTransport
import dev.bridgething.schema.AuthorityClaim
import dev.bridgething.schema.AuthorityRelease
import dev.bridgething.schema.CompanionAuthorityScope
import dev.bridgething.schema.PlaybackState
import dev.bridgething.schema.PlayerState
import dev.bridgething.schema.QueueSnapshot
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch

/**
 * The single companion-owned now-playing arbiter + emitter. Sources push player
 * snapshots / queues here; the hub is the ONLY caller of `gateway.player.snapshot`,
 * `gateway.player.queueChanged`, and the now-playing authority claim/release.
 */
class NowPlayingHub(private val gateway: BridgethingGateway) : NowPlayingSink {
    private class SourceState {
        var snapshot: PlayerState? = null
        var appBundle: String = ""
        var hasItem: Boolean = false
        var queue: QueueSnapshot? = null
        var seq: Long = 0
    }

    private sealed interface Op {
        data class Player(val sourceId: String, val snapshot: PlayerState, val appBundle: String, val hasItem: Boolean) : Op
        data class Queue(val sourceId: String, val queue: QueueSnapshot) : Op
        data class Clear(val sourceId: String) : Op
        object Reconnect : Op
    }

    private val channel = Channel<Op>(Channel.UNLIMITED)
    private var consumer: Job? = null

    private val sources = HashMap<String, SourceState>()
    private var seqCounter: Long = 0
    @Volatile private var current: String? = null
    private val heldScopes = mutableSetOf<CompanionAuthorityScope>()
    private var claimedBundle: String? = null

    private val transports = java.util.concurrent.ConcurrentHashMap<String, NowPlayingTransport>()

    fun start(scope: CoroutineScope) {
        if (consumer != null) return
        consumer = scope.launch {
            for (op in channel) handle(op)
        }
    }

    fun stop() {
        consumer?.cancel()
        consumer = null
    }

    override fun submitPlayer(sourceId: String, snapshot: PlayerState, appBundle: String, hasItem: Boolean) {
        channel.trySend(Op.Player(sourceId, snapshot, appBundle, hasItem))
    }

    override fun submitQueue(sourceId: String, queue: QueueSnapshot) {
        channel.trySend(Op.Queue(sourceId, queue))
    }

    override fun clearSource(sourceId: String) {
        channel.trySend(Op.Clear(sourceId))
    }

    /** A device peer (re)connected; the daemon dropped authority, so re-claim + re-emit the current source. */
    fun onConnect() {
        channel.trySend(Op.Reconnect)
    }

    /** Register a source's control surface so inbound transport verbs can route to it when it is audible. */
    fun register(sourceId: String, transport: NowPlayingTransport) {
        transports[sourceId] = transport
    }

    fun unregister(sourceId: String) {
        transports.remove(sourceId)
    }

    /** The control surface of the arbitrated current source, or null when nothing is playing / it is unregistered. */
    fun currentTransport(): NowPlayingTransport? = current?.let { transports[it] }

    private suspend fun handle(op: Op) {
        when (op) {
            is Op.Player -> {
                val s = sources.getOrPut(op.sourceId) { SourceState() }
                s.snapshot = op.snapshot
                s.appBundle = op.appBundle
                s.hasItem = op.hasItem
                s.seq = ++seqCounter
                emitArbitrated()
            }
            is Op.Queue -> {
                val s = sources.getOrPut(op.sourceId) { SourceState() }
                s.queue = op.queue
                if (current == null || current == op.sourceId) {
                    runCatching { gateway.player.queueChanged(op.queue) }
                }
            }
            is Op.Clear -> {
                sources.remove(op.sourceId)
                if (op.sourceId == current) {
                    current = null
                    emitArbitrated()
                }
            }
            Op.Reconnect -> {
                heldScopes.clear()
                claimedBundle = null
                reemitCurrent()
            }
        }
    }

    private suspend fun emitArbitrated() {
        val prev = current
        val next = pickCurrent()
        current = next
        if (next == null) {
            releaseAll()
            return
        }
        val s = sources.getValue(next)
        if (s.hasItem) claim(s.appBundle) else releaseAll()
        s.snapshot?.let { runCatching { gateway.player.snapshot(it) } }
        if (prev != null && next != prev) {
            runCatching { gateway.player.queueChanged(s.queue ?: QueueSnapshot(order = emptyList(), items = emptyList())) }
        }
    }

    private suspend fun reemitCurrent() {
        val next = current ?: pickCurrent()?.also { current = it } ?: return
        val s = sources[next] ?: return
        if (s.hasItem) claim(s.appBundle)
        s.snapshot?.let { runCatching { gateway.player.snapshot(it) } }
        s.queue?.let { runCatching { gateway.player.queueChanged(it) } }
    }

    private fun pickCurrent(): String? {
        if (sources.isEmpty()) return null
        val playing = sources.filter { it.value.hasItem && it.value.snapshot?.playback?.state == PlaybackState.Playing }
        val pool = if (playing.isNotEmpty()) playing else sources
        return pool.maxByOrNull { it.value.seq }?.key
    }

    private suspend fun claim(appBundle: String) {
        val bundleChanged = claimedBundle != appBundle
        for (scope in NOW_PLAYING_SCOPES) {
            if (!heldScopes.contains(scope) || bundleChanged) {
                val sent = runCatching { gateway.authority.claim(AuthorityClaim(scope = scope, appBundle = appBundle)) }.isSuccess
                if (sent) heldScopes.add(scope)
            }
        }
        claimedBundle = appBundle
    }

    private suspend fun releaseAll() {
        claimedBundle = null
        if (heldScopes.isEmpty()) return
        val scopes = heldScopes.toList()
        heldScopes.clear()
        for (scope in scopes) {
            runCatching { gateway.authority.release(AuthorityRelease(scope = scope)) }
        }
    }

    private companion object {
        val NOW_PLAYING_SCOPES = listOf(
            CompanionAuthorityScope.NowPlayingPlayback,
            CompanionAuthorityScope.NowPlayingMetadata,
        )
    }
}
