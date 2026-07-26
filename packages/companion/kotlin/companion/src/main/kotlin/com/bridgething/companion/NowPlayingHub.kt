package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.authority
import com.bridgething.gateway.player
import com.bridgething.glue.NowPlayingSink
import com.bridgething.glue.NowPlayingTransport
import com.bridgething.schema.AuthorityClaim
import com.bridgething.schema.AuthorityRelease
import com.bridgething.schema.CompanionAuthorityScope
import com.bridgething.schema.PlaybackState
import com.bridgething.schema.PlaybackTargets
import com.bridgething.schema.PlayerState
import com.bridgething.schema.QueueSnapshot
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch

class NowPlayingHub(private val gateway: BridgethingGateway) : NowPlayingSink {
    private class SourceState {
        var snapshot: PlayerState? = null
        var appBundle: String = ""
        var hasItem: Boolean = false
        var wantsVolume: Boolean = false
        var queue: QueueSnapshot? = null
        var targets: PlaybackTargets? = null
        var seq: Long = 0
    }

    private sealed interface Op {
        data class Player(
            val sourceId: String,
            val snapshot: PlayerState,
            val appBundle: String,
            val hasItem: Boolean,
            val wantsVolume: Boolean,
        ) : Op
        data class Queue(val sourceId: String, val queue: QueueSnapshot) : Op
        data class Targets(val sourceId: String, val targets: PlaybackTargets) : Op
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

    override fun submitPlayer(
        sourceId: String,
        snapshot: PlayerState,
        appBundle: String,
        hasItem: Boolean,
        wantsVolume: Boolean,
    ) {
        channel.trySend(Op.Player(sourceId, snapshot, appBundle, hasItem, wantsVolume))
    }

    override fun submitQueue(sourceId: String, queue: QueueSnapshot) {
        channel.trySend(Op.Queue(sourceId, queue))
    }

    override fun submitTargets(sourceId: String, targets: PlaybackTargets) {
        channel.trySend(Op.Targets(sourceId, targets))
    }

    override fun clearSource(sourceId: String) {
        channel.trySend(Op.Clear(sourceId))
    }

    fun onConnect() {
        channel.trySend(Op.Reconnect)
    }

    fun register(sourceId: String, transport: NowPlayingTransport) {
        transports[sourceId] = transport
    }

    fun unregister(sourceId: String) {
        transports.remove(sourceId)
    }

    fun currentTransport(): NowPlayingTransport? = current?.let { transports[it] }

    fun currentSource(): String? = current

    private suspend fun handle(op: Op) {
        when (op) {
            is Op.Player -> {
                val s = sources.getOrPut(op.sourceId) { SourceState() }
                s.snapshot = op.snapshot
                s.appBundle = op.appBundle
                s.hasItem = op.hasItem
                s.wantsVolume = op.wantsVolume
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
            is Op.Targets -> {
                val s = sources.getOrPut(op.sourceId) { SourceState() }
                s.targets = op.targets
                if (current == null || current == op.sourceId) {
                    runCatching { gateway.player.targetsChanged(op.targets) }
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
        if (s.hasItem) claim(s.appBundle, s.wantsVolume) else releaseAll()
        s.snapshot?.let { runCatching { gateway.player.snapshot(it) } }
        if (prev != null && next != prev) {
            runCatching { gateway.player.queueChanged(s.queue ?: QueueSnapshot(order = emptyList(), items = emptyList())) }
            runCatching { gateway.player.targetsChanged(s.targets ?: PlaybackTargets(targets = emptyList())) }
        }
    }

    private suspend fun reemitCurrent() {
        val next = current ?: pickCurrent()?.also { current = it } ?: return
        val s = sources[next] ?: return
        if (s.hasItem) claim(s.appBundle, s.wantsVolume)
        s.snapshot?.let { runCatching { gateway.player.snapshot(it) } }
        s.queue?.let { runCatching { gateway.player.queueChanged(it) } }
        s.targets?.let { runCatching { gateway.player.targetsChanged(it) } }
    }

    private fun pickCurrent(): String? {
        if (sources.isEmpty()) return null
        val playing = sources.filter { it.value.hasItem && it.value.snapshot?.playback?.state == PlaybackState.Playing }
        val pool = if (playing.isNotEmpty()) playing else sources
        return pool.maxByOrNull { it.value.seq }?.key
    }

    private suspend fun claim(appBundle: String, wantsVolume: Boolean) {
        val bundleChanged = claimedBundle != appBundle
        val want = if (wantsVolume) NOW_PLAYING_SCOPES + CompanionAuthorityScope.Volume else NOW_PLAYING_SCOPES
        for (scope in want) {
            if (!heldScopes.contains(scope) || bundleChanged) {
                val sent = runCatching { gateway.authority.claim(AuthorityClaim(scope = scope, appBundle = appBundle)) }.isSuccess
                if (sent) heldScopes.add(scope)
            }
        }
        if (!wantsVolume && heldScopes.remove(CompanionAuthorityScope.Volume)) {
            runCatching { gateway.authority.release(AuthorityRelease(scope = CompanionAuthorityScope.Volume)) }
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
