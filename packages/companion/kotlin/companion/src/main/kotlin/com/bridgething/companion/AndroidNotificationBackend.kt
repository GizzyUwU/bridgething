package com.bridgething.companion

import android.app.PendingIntent
import com.bridgething.schema.Notification as WireNotification
import com.bridgething.schema.NotificationRemoved
import com.bridgething.schema.NotificationsError
import com.bridgething.schema.NotificationsErrorActionRejectedInner
import com.bridgething.schema.NotificationsErrorNotFoundInner
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow

public class AndroidNotificationBackend(
    private val resolveAction: (id: String, positive: Boolean) -> PendingIntent?,
) : NotificationBackend {
    private val _events = MutableSharedFlow<NotificationOutEvent>(extraBufferCapacity = 64)
    override val events: Flow<NotificationOutEvent> = _events.asSharedFlow()

    override suspend fun invokePositive(id: String): NotificationsError? = fire(id, positive = true)

    override suspend fun invokeNegative(id: String): NotificationsError? = fire(id, positive = false)

    public fun emitPosted(notification: WireNotification) {
        _events.tryEmit(NotificationOutEvent.Posted(notification))
    }

    public fun emitRemoved(removed: NotificationRemoved) {
        _events.tryEmit(NotificationOutEvent.Removed(removed))
    }

    private fun fire(id: String, positive: Boolean): NotificationsError? {
        val intent = runCatching { resolveAction(id, positive) }.getOrNull()
            ?: return NotificationsError.NotFound(NotificationsErrorNotFoundInner(id))
        return runCatching { intent.send() }.fold(
            onSuccess = { null },
            onFailure = {
                NotificationsError.ActionRejected(
                    NotificationsErrorActionRejectedInner(it.message ?: it.toString()),
                )
            },
        )
    }
}
