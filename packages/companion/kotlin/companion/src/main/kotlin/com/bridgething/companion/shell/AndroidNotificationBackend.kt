package com.bridgething.companion.shell

import android.app.PendingIntent
import uniffi.bridgething_companion.ActionSink
import uniffi.bridgething_companion.NotificationActionError
import uniffi.bridgething_companion.NotificationBackend
import uniffi.bridgething_companion.NotificationInbox
import uniffi.bridgething_companion.NotificationRemoved
import uniffi.bridgething_companion.WireNotification

public class AndroidNotificationBackend(
    private val resolveAction: (id: String, positive: Boolean) -> PendingIntent?,
) : NotificationBackend {
    @Volatile
    private var inbox: NotificationInbox? = null

    override fun start(inbox: NotificationInbox) {
        val previous = this.inbox
        this.inbox = inbox
        previous?.close()
    }

    override fun stop() {
        val previous = inbox
        inbox = null
        previous?.close()
    }

    override fun invokePositive(id: String, sink: ActionSink) {
        fire(id, positive = true, sink)
    }

    override fun invokeNegative(id: String, sink: ActionSink) {
        fire(id, positive = false, sink)
    }

    public fun emitPosted(notification: WireNotification) {
        inbox?.let { runCatching { it.onPosted(notification) } }
    }

    public fun emitRemoved(removed: NotificationRemoved) {
        inbox?.let { runCatching { it.onRemoved(removed) } }
    }

    private fun fire(id: String, positive: Boolean, sink: ActionSink) {
        sink.use {
            val intent = runCatching { resolveAction(id, positive) }.getOrNull()
            if (intent == null) {
                it.fail(NotificationActionError.NotFound(id))
                return
            }
            runCatching { intent.send() }.fold(
                onSuccess = { _ -> it.complete() },
                onFailure = { t -> it.fail(NotificationActionError.ActionRejected(t.message ?: t.toString())) },
            )
        }
    }
}
