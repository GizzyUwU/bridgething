package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.phone
import com.bridgething.schema.CommunicationsSnapshot
import com.bridgething.schema.PhoneStateReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

public class PhoneDispatcher(
    private val backend: PhoneBackend,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val mutex = Mutex()
    private val jobs = mutableListOf<Job>()

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            stopJobs()
            jobs.add(scope.launch { gateway.phone.answer.collect { (_, msg) -> backend.answer(msg.callId) } })
            jobs.add(scope.launch { gateway.phone.accept.collect { (_, msg) -> backend.accept(msg.callId, msg.action) } })
            jobs.add(scope.launch { gateway.phone.decline.collect { (_, msg) -> backend.decline(msg.callId) } })
            jobs.add(scope.launch { gateway.phone.end.collect { (_, msg) -> backend.end(msg.callId) } })
            jobs.add(scope.launch { gateway.phone.endTyped.collect { (_, msg) -> backend.endTyped(msg.callId, msg.action) } })
            jobs.add(scope.launch { gateway.phone.hold.collect { (_, msg) -> backend.hold(msg.callId) } })
            jobs.add(scope.launch { gateway.phone.unhold.collect { (_, msg) -> backend.unhold(msg.callId) } })
            jobs.add(scope.launch { gateway.phone.initiate.collect { (_, msg) -> backend.initiate(msg) } })
            jobs.add(scope.launch { gateway.phone.swap.collect { backend.swap() } })
            jobs.add(scope.launch { gateway.phone.merge.collect { backend.merge() } })
            jobs.add(scope.launch { gateway.phone.mute.collect { (_, msg) -> backend.mute(msg.mute) } })
            jobs.add(scope.launch { gateway.phone.dtmf.collect { (_, msg) -> backend.dtmf(msg.callId, msg.tone) } })
            jobs.add(scope.launch { gateway.phone.stateGetRequests.collect { handle -> handle.respond(PhoneStateReply(backend.stateGet())) } })
            jobs.add(scope.launch { relayEvents(gateway) })
        }
    }

    // seeds the daemon with the live call set so a reconnecting peer does not miss in-progress calls
    public suspend fun announce(gateway: BridgethingGateway) {
        runCatching { gateway.phone.snapshot(PhoneStateReply(backend.stateGet())) }
    }

    public suspend fun stop() {
        mutex.withLock { stopJobs() }
    }

    public fun close() {
        scope.cancel()
    }

    private fun stopJobs() {
        for (job in jobs) job.cancel()
        jobs.clear()
    }

    private suspend fun relayEvents(gateway: BridgethingGateway) {
        backend.events.collect { event ->
            runCatching {
                when (event) {
                    is PhoneOutEvent.CallStarted -> gateway.phone.callStarted(event.call)
                    is PhoneOutEvent.CallUpdated -> gateway.phone.callUpdated(event.call)
                    is PhoneOutEvent.CallEnded -> gateway.phone.callEnded(event.ended)
                    is PhoneOutEvent.Snapshot -> gateway.phone.snapshot(PhoneStateReply(event.state))
                    is PhoneOutEvent.Communications -> gateway.phone.communicationsSnapshot(CommunicationsSnapshot(event.state))
                }
            }
        }
    }
}
