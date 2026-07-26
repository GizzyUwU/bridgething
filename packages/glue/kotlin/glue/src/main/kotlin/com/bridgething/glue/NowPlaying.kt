package com.bridgething.glue

import com.bridgething.schema.PlaybackTargets
import com.bridgething.schema.PlayerState
import com.bridgething.schema.QueueSnapshot

interface NowPlayingSink {
    fun submitPlayer(
        sourceId: String,
        snapshot: PlayerState,
        appBundle: String,
        hasItem: Boolean,
        wantsVolume: Boolean = false,
    )
    fun submitQueue(sourceId: String, queue: QueueSnapshot)
    fun submitTargets(sourceId: String, targets: PlaybackTargets)
    fun clearSource(sourceId: String)
}
