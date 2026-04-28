package dev.bridgething.gateway

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.consumeAsFlow

/**
 * Test-only [Adapter] that lets a test drive byte/connection events into the
 * gateway and pull back outbound frames. The events flow is backed by an
 * unlimited Channel so simulate() before the gateway has had a chance to
 * subscribe still gets buffered — a SharedFlow with no replay drops events
 * for a not-yet-attached subscriber, which is exactly the race that bit the
 * forwards-events test on the first run.
 */
internal class MockAdapter : Adapter {
  private val incomingEvents: Channel<AdapterEvent> = Channel(Channel.UNLIMITED)
  override val events: Flow<AdapterEvent> = incomingEvents.consumeAsFlow()

  val sentFrames: Channel<Pair<String, ByteArray>> = Channel(Channel.UNLIMITED)

  var startCalled: Boolean = false
  var stopCalled: Boolean = false
  val disconnectCalls: MutableList<String> = mutableListOf()

  override suspend fun start() { startCalled = true }
  override suspend fun stop() {
    stopCalled = true
    incomingEvents.close()
    sentFrames.close()
  }
  override suspend fun disconnect(deviceId: String) {
    disconnectCalls.add(deviceId)
  }
  override suspend fun send(deviceId: String, frame: ByteArray) {
    sentFrames.send(deviceId to frame)
  }

  suspend fun simulate(event: AdapterEvent) {
    incomingEvents.send(event)
  }
}
