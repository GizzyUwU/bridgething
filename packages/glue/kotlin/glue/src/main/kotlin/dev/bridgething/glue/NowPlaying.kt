package dev.bridgething.glue

import dev.bridgething.schema.PlayerState
import dev.bridgething.schema.QueueSnapshot

/**
 * Sink a now-playing source pushes to. The companion's `NowPlayingHub` implements
 * it and is the SOLE emitter of `gateway.player.snapshot` / `queueChanged` and the
 * now-playing authority claim/release: sources never call the gateway directly, so
 * the single snapshot stream and single last-writer-wins `app_bundle` slot the
 * daemon exposes are arbitrated in one place (the daemon cannot arbitrate two
 * companion sources itself - the iAP2 lane that does that on iOS is inert on
 * Android).
 *
 * Calls are non-suspending so a source can submit from a synchronous firehose
 * callback without launching a coroutine: the hub enqueues onto one ordered
 * channel, so submissions from one source stay in push order (a launched submit
 * would reorder claim/release/snapshot for rapid pushes).
 *
 * `sourceId` is a stable per-source key (e.g. `"spotify"`); `appBundle` is the
 * bundle the source represents, claimed on the daemon when this source is the
 * audible one.
 */
interface NowPlayingSink {
    fun submitPlayer(sourceId: String, snapshot: PlayerState, appBundle: String, hasItem: Boolean)
    fun submitQueue(sourceId: String, queue: QueueSnapshot)
    fun clearSource(sourceId: String)
}
