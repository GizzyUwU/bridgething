package com.bridgething.companion

import com.bridgething.gateway.Adapter
import com.bridgething.gateway.AdapterEvent
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.consumeAsFlow

class FakeAdapter : Adapter {
    private val incoming = Channel<AdapterEvent>(Channel.UNLIMITED)
    override val events: Flow<AdapterEvent> = incoming.consumeAsFlow()

    val sentFrames = Channel<Pair<String, ByteArray>>(Channel.UNLIMITED)

    override suspend fun start() {}

    override suspend fun stop() {
        incoming.close()
        sentFrames.close()
    }

    override suspend fun disconnect(deviceId: String) {}

    override suspend fun send(deviceId: String, frame: ByteArray) {
        sentFrames.send(deviceId to frame)
    }

    suspend fun simulate(event: AdapterEvent) {
        incoming.send(event)
    }
}
