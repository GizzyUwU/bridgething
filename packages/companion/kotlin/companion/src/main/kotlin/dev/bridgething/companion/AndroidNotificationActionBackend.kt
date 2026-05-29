package dev.bridgething.companion

import android.app.PendingIntent

// `positive` maps to a notification's first action slot, `negative` to its second (e.g. Answer/Decline)
public class AndroidNotificationActionBackend(
    private val resolveAction: (id: String, positive: Boolean) -> PendingIntent?,
) : NotificationActionBackend {
    public override suspend fun invokePositive(id: String) {
        fire(id, positive = true)
    }

    public override suspend fun invokeNegative(id: String) {
        fire(id, positive = false)
    }

    private fun fire(id: String, positive: Boolean) {
        val intent = resolveAction(id, positive) ?: return
        runCatching { intent.send() }
    }
}
