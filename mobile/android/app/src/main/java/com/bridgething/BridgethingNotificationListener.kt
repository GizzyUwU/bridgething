package com.bridgething

import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.pm.PackageManager
import android.service.notification.NotificationListenerService
import android.service.notification.NotificationListenerService.Ranking
import android.service.notification.StatusBarNotification
import android.util.Log
import com.bridgething.companion.BridgethingCompanion
import com.bridgething.companion.shell.AndroidNotificationBackend
import uniffi.bridgething_companion.DismissReason
import uniffi.bridgething_companion.NotificationAction
import uniffi.bridgething_companion.NotificationApp
import uniffi.bridgething_companion.NotificationCategory
import uniffi.bridgething_companion.NotificationFlags
import uniffi.bridgething_companion.NotificationRemoved
import uniffi.bridgething_companion.WireNotification

public class BridgethingNotificationListener : NotificationListenerService() {

    override fun onListenerConnected() {
        super.onListenerConnected()
        NotificationBridgeRegistry.listener = this
        NotificationBridgeRegistry.companion?.mediaSessions?.refresh()
        Log.i(TAG, "notification listener connected")
    }

    override fun onListenerDisconnected() {
        super.onListenerDisconnected()
        if (NotificationBridgeRegistry.listener === this) NotificationBridgeRegistry.listener = null
        Log.i(TAG, "notification listener disconnected")
    }

    override fun onDestroy() {
        if (NotificationBridgeRegistry.listener === this) NotificationBridgeRegistry.listener = null
        super.onDestroy()
    }

    fun actionIntent(id: String, positive: Boolean): PendingIntent? {
        val sbn = activeNotifications?.firstOrNull { it.key == id } ?: return null
        return sbn.notification?.actions?.getOrNull(if (positive) 0 else 1)?.actionIntent
    }

    override fun onNotificationPosted(sbn: StatusBarNotification?) {
        val sbnIt = sbn ?: return
        if (shouldSkip(sbnIt)) return
        NotificationBridgeRegistry.backend?.emitPosted(toWireNotification(sbnIt))
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification?, rankingMap: RankingMap?, reason: Int) {
        val sbnIt = sbn ?: return
        if (shouldSkip(sbnIt)) return
        val dismissReason = when (reason) {
            REASON_APP_CANCEL, REASON_APP_CANCEL_ALL, REASON_LISTENER_CANCEL, REASON_LISTENER_CANCEL_ALL ->
                DismissReason.REMOTE_DISMISSED
            REASON_CLICK -> DismissReason.ACTED
            else -> DismissReason.USER_DISMISSED
        }
        NotificationBridgeRegistry.backend?.emitRemoved(NotificationRemoved(id = sbnIt.key, reason = dismissReason))
    }

    private fun shouldSkip(sbn: StatusBarNotification): Boolean {
        if (sbn.packageName == applicationContext.packageName) return true
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

        val importance = channelImportance(sbn.key)
        val flags = NotificationFlags(
            silent = importance < NotificationManager.IMPORTANCE_DEFAULT,
            important = importance >= NotificationManager.IMPORTANCE_HIGH,
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

    private fun channelImportance(key: String): Int = runCatching {
        val ranking = Ranking()
        val imp = if (currentRanking.getRanking(key, ranking)) ranking.importance else NotificationManager.IMPORTANCE_DEFAULT
        if (imp < NotificationManager.IMPORTANCE_NONE) NotificationManager.IMPORTANCE_DEFAULT else imp
    }.getOrDefault(NotificationManager.IMPORTANCE_DEFAULT)

    private fun mapCategory(raw: String?): NotificationCategory = when (raw) {
        Notification.CATEGORY_CALL -> NotificationCategory.INCOMING_CALL
        Notification.CATEGORY_MISSED_CALL -> NotificationCategory.MISSED_CALL
        Notification.CATEGORY_VOICEMAIL -> NotificationCategory.VOICEMAIL
        Notification.CATEGORY_MESSAGE, Notification.CATEGORY_SOCIAL -> NotificationCategory.SOCIAL
        Notification.CATEGORY_REMINDER, Notification.CATEGORY_EVENT -> NotificationCategory.SCHEDULE
        Notification.CATEGORY_EMAIL -> NotificationCategory.EMAIL
        Notification.CATEGORY_NAVIGATION, Notification.CATEGORY_LOCATION_SHARING -> NotificationCategory.LOCATION
        Notification.CATEGORY_RECOMMENDATION, Notification.CATEGORY_PROMO -> NotificationCategory.NEWS
        else -> NotificationCategory.OTHER
    }

    private companion object {
        const val TAG = "bridgething.notif"
    }
}

public object NotificationBridgeRegistry {
    @Volatile
    public var companion: BridgethingCompanion? = null

    @Volatile
    public var backend: AndroidNotificationBackend? = null

    @Volatile
    public var listener: BridgethingNotificationListener? = null
}
