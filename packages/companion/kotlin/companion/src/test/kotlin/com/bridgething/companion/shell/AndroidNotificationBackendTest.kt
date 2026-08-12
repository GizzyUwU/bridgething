package com.bridgething.companion.shell

import android.app.PendingIntent
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.ActionSink
import uniffi.bridgething_companion.DismissReason
import uniffi.bridgething_companion.NoHandle
import uniffi.bridgething_companion.NotificationActionError
import uniffi.bridgething_companion.NotificationApp
import uniffi.bridgething_companion.NotificationCategory
import uniffi.bridgething_companion.NotificationFlags
import uniffi.bridgething_companion.NotificationInbox
import uniffi.bridgething_companion.NotificationRemoved
import uniffi.bridgething_companion.WireNotification

private class RecordingActionSink : ActionSink(NoHandle) {
    val outcomes = LinkedBlockingQueue<Any>()

    override fun complete() {
        outcomes.add("ok")
    }

    override fun fail(error: NotificationActionError) {
        outcomes.add(error)
    }
}

private class RecordingNotificationInbox : NotificationInbox(NoHandle) {
    val events = LinkedBlockingQueue<Any>()

    override fun onPosted(notification: WireNotification) {
        events.add(notification)
    }

    override fun onRemoved(removed: NotificationRemoved) {
        events.add(removed)
    }
}

class AndroidNotificationBackendTest {
    private fun notification(id: String) = WireNotification(
        id = id,
        app = NotificationApp(bundleId = "com.example", displayName = "Example", iconAssetId = null),
        category = NotificationCategory.SOCIAL,
        title = "hi",
        subtitle = null,
        message = "there",
        timestampUnixS = null,
        flags = NotificationFlags(silent = false, important = false),
        positiveAction = null,
        negativeAction = null,
    )

    @Test
    fun postedAndRemovedReachTheInboxOnceStarted() {
        val backend = AndroidNotificationBackend { _, _ -> null }
        val inbox = RecordingNotificationInbox()
        backend.start(inbox)

        backend.emitPosted(notification("n1"))
        assertEquals(notification("n1"), inbox.events.poll(1, TimeUnit.SECONDS))

        backend.emitRemoved(NotificationRemoved(id = "n1", reason = DismissReason.USER_DISMISSED))
        assertEquals(NotificationRemoved("n1", DismissReason.USER_DISMISSED), inbox.events.poll(1, TimeUnit.SECONDS))

        backend.stop()
        backend.emitPosted(notification("n2"))
        assertTrue(inbox.events.isEmpty(), "a stopped backend reports nothing")
    }

    @Test
    fun aResolvedActionFiresAndCompletes() {
        val intent = mockk<PendingIntent>(relaxed = true)
        val backend = AndroidNotificationBackend { id, positive ->
            if (id == "n1" && positive) intent else null
        }
        val sink = RecordingActionSink()
        backend.invokePositive("n1", sink)
        assertEquals("ok", sink.outcomes.poll(1, TimeUnit.SECONDS))
        verify { intent.send() }
    }

    @Test
    fun anUnknownIdFailsNotFound() {
        val backend = AndroidNotificationBackend { _, _ -> null }
        val sink = RecordingActionSink()
        backend.invokeNegative("missing", sink)
        assertEquals(NotificationActionError.NotFound("missing"), sink.outcomes.poll(1, TimeUnit.SECONDS))
    }

    @Test
    fun aThrowingSendFailsActionRejected() {
        val intent = mockk<PendingIntent>()
        every { intent.send() } throws PendingIntent.CanceledException("gone")
        val backend = AndroidNotificationBackend { _, _ -> intent }
        val sink = RecordingActionSink()
        backend.invokePositive("n1", sink)
        val outcome = sink.outcomes.poll(1, TimeUnit.SECONDS)
        assertTrue(outcome is NotificationActionError.ActionRejected, "got $outcome")
    }
}
