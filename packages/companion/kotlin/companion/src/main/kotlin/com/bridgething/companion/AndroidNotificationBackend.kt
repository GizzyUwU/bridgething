package com.bridgething.companion

import android.app.PendingIntent
import com.bridgething.schema.Notification as WireNotification
import com.bridgething.schema.NotificationRemoved
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/**
 * Real [NotificationBackend] bridging the host's NotificationListenerService (which the OS constructs
 * independently of the host app) to the companion. The listener pushes posted/removed events in via
 * [emitPosted] / [emitRemoved] and supplies the shade + action resolver as host lambdas, so this stays in
 * the companion library while the OS service stays in the host. `positive` is a notification's first action
 * slot, `negative` its second (e.g. Answer/Decline).
 */
public class AndroidNotificationBackend(
    private val activeShade: () -> List<WireNotification>,
    private val resolveAction: (id: String, positive: Boolean) -> PendingIntent?,
) : NotificationBackend {
    private val _events = MutableSharedFlow<NotificationOutEvent>(extraBufferCapacity = 64)
    override val events: Flow<NotificationOutEvent> = _events.asSharedFlow()

    override fun activeNotifications(): List<WireNotification> = runCatching { activeShade() }.getOrDefault(emptyList())

    override suspend fun invokePositive(id: String) {
        fire(id, positive = true)
    }

    override suspend fun invokeNegative(id: String) {
        fire(id, positive = false)
    }

    public fun emitPosted(notification: WireNotification) {
        _events.tryEmit(NotificationOutEvent.Posted(notification))
    }

    public fun emitRemoved(removed: NotificationRemoved) {
        _events.tryEmit(NotificationOutEvent.Removed(removed))
    }

    private fun fire(id: String, positive: Boolean) {
        val intent = runCatching { resolveAction(id, positive) }.getOrNull() ?: return
        runCatching { intent.send() }
    }
}
