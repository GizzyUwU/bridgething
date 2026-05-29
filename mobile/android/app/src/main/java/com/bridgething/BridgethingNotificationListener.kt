package com.bridgething

import android.app.Notification
import android.app.PendingIntent
import android.content.pm.PackageManager
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Log
import dev.bridgething.companion.BridgethingCompanion
import dev.bridgething.gateway.notifications
import dev.bridgething.schema.DismissReason
import dev.bridgething.schema.NotificationAction
import dev.bridgething.schema.NotificationApp
import dev.bridgething.schema.NotificationCategory
import dev.bridgething.schema.NotificationFlags
import dev.bridgething.schema.NotificationRemoved
import dev.bridgething.schema.Notification as WireNotification
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Forwards notifications to every connected Car Thing via the running companion's gateway.
 * The OS constructs this service independently of the host app, so the companion is looked
 * up through [NotificationBridgeRegistry] rather than injected directly.
 */
public class BridgethingNotificationListener : NotificationListenerService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onListenerConnected() {
        super.onListenerConnected()
        NotificationBridgeRegistry.listener = this
        Log.i(TAG, "notification listener connected")
    }

    override fun onListenerDisconnected() {
        super.onListenerDisconnected()
        if (NotificationBridgeRegistry.listener === this) NotificationBridgeRegistry.listener = null
        Log.i(TAG, "notification listener disconnected")
    }

    override fun onDestroy() {
        if (NotificationBridgeRegistry.listener === this) NotificationBridgeRegistry.listener = null
        scope.cancel()
        super.onDestroy()
    }

    fun actionIntent(id: String, positive: Boolean): PendingIntent? {
        val sbn = activeNotifications?.firstOrNull { it.key == id } ?: return null
        return sbn.notification?.actions?.getOrNull(if (positive) 0 else 1)?.actionIntent
    }

    override fun onNotificationPosted(sbn: StatusBarNotification?) {
        val sbnIt = sbn ?: return
        if (shouldSkip(sbnIt)) return
        val companion = NotificationBridgeRegistry.companion ?: return
        val wire = toWireNotification(sbnIt)
        scope.launch {
            runCatching { companion.gateway.notifications.posted(wire) }
                .onFailure { Log.w(TAG, "notifications.posted failed: ${it.message}") }
        }
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification?, rankingMap: RankingMap?, reason: Int) {
        val sbnIt = sbn ?: return
        if (shouldSkip(sbnIt)) return
        val companion = NotificationBridgeRegistry.companion ?: return
        val dismissReason = if (reason == REASON_CLICK) DismissReason.Acted else DismissReason.UserDismissed
        scope.launch {
            runCatching {
                companion.gateway.notifications.removed(
                    NotificationRemoved(id = sbnIt.key, reason = dismissReason)
                )
            }.onFailure { Log.w(TAG, "notifications.removed failed: ${it.message}") }
        }
    }

    private fun shouldSkip(sbn: StatusBarNotification): Boolean {
        // skip our own package to prevent looping notifications back to the device.
        if (sbn.packageName == applicationContext.packageName) return true
        // group summaries and ongoing events (media, foreground services) are not user-facing alerts.
        val n = sbn.notification ?: return true
        if ((n.flags and Notification.FLAG_GROUP_SUMMARY) != 0) return true
        if ((n.flags and Notification.FLAG_ONGOING_EVENT) != 0) return true
        return false
    }

    private val appLabelCache = java.util.concurrent.ConcurrentHashMap<String, String>()

    private fun resolveAppLabel(packageName: String): String? = appLabelCache.getOrPut(packageName) {
        try {
            val pm = packageManager
            pm.getApplicationLabel(pm.getApplicationInfo(packageName, 0)).toString()
        } catch (_: PackageManager.NameNotFoundException) {
            ""
        }
    }.takeIf { it.isNotEmpty() }

    private fun toWireNotification(sbn: StatusBarNotification): WireNotification {
        val n = sbn.notification
        val extras = n.extras
        val title = extras?.let {
            it.getString(Notification.EXTRA_TITLE) ?: it.getCharSequence(Notification.EXTRA_TITLE)?.toString()
        }
        val text = extras?.getCharSequence(Notification.EXTRA_TEXT)?.toString()
        val subText = extras?.getCharSequence(Notification.EXTRA_SUB_TEXT)?.toString()
        val displayName = resolveAppLabel(sbn.packageName)

        // n.priority is deprecated post-api-26 but still populated by legacy notifications; channel
        // importance lookup would need NotificationManager round-trips per event.
        @Suppress("DEPRECATION")
        val flags = NotificationFlags(
            silent = (n.flags and Notification.FLAG_NO_CLEAR) != 0,
            important = n.priority >= Notification.PRIORITY_HIGH,
            preExisting = false,
        )

        val actions = n.actions
        fun actionSlot(index: Int): NotificationAction? =
            actions?.getOrNull(index)?.title?.toString()?.takeIf { it.isNotEmpty() }?.let { NotificationAction(label = it) }

        return WireNotification(
            id = sbn.key,
            app = NotificationApp(
                bundleId = sbn.packageName,
                displayName = displayName,
                iconAssetId = null,
            ),
            category = mapCategory(n.category),
            title = title,
            subtitle = subText,
            message = text,
            timestampUnixS = (sbn.postTime / 1000L).coerceIn(0L, UInt.MAX_VALUE.toLong()).toUInt(),
            flags = flags,
            positiveAction = actionSlot(0),
            negativeAction = actionSlot(1),
        )
    }

    private fun mapCategory(raw: String?): NotificationCategory = when (raw) {
        Notification.CATEGORY_CALL -> NotificationCategory.IncomingCall
        Notification.CATEGORY_MISSED_CALL -> NotificationCategory.MissedCall
        Notification.CATEGORY_VOICEMAIL -> NotificationCategory.Voicemail
        Notification.CATEGORY_MESSAGE, Notification.CATEGORY_SOCIAL -> NotificationCategory.Social
        Notification.CATEGORY_REMINDER, Notification.CATEGORY_EVENT -> NotificationCategory.Schedule
        Notification.CATEGORY_EMAIL -> NotificationCategory.Email
        Notification.CATEGORY_NAVIGATION, Notification.CATEGORY_LOCATION_SHARING -> NotificationCategory.Location
        Notification.CATEGORY_RECOMMENDATION, Notification.CATEGORY_PROMO -> NotificationCategory.News
        else -> NotificationCategory.Other
    }

    private companion object {
        const val TAG = "bridgething.notif"
    }
}

/** bridges the OS-constructed listener to the running companion. */
public object NotificationBridgeRegistry {
    @Volatile
    public var companion: BridgethingCompanion? = null

    @Volatile
    public var listener: BridgethingNotificationListener? = null
}
