package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.notifications
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

public class NotificationDispatcher(
    private val backend: NotificationBackend,
    private val enabled: () -> Boolean,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val mutex = Mutex()
    private val jobs = mutableListOf<Job>()

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            stopJobs()
            jobs.add(scope.launch { relayEvents(gateway) })
            jobs.add(scope.launch { gateway.notifications.invokePositive.collect { (_, msg) -> backend.invokePositive(msg.id) } })
            jobs.add(scope.launch { gateway.notifications.invokeNegative.collect { (_, msg) -> backend.invokeNegative(msg.id) } })
        }
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
            if (!enabled()) return@collect
            runCatching {
                when (event) {
                    is NotificationOutEvent.Posted -> gateway.notifications.posted(event.notification)
                    is NotificationOutEvent.Removed -> gateway.notifications.removed(event.removed)
                }
            }
        }
    }
}
