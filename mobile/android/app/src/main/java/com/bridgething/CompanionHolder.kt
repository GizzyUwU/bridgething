package com.bridgething

import android.app.Activity
import android.app.Application
import android.bluetooth.BluetoothManager
import android.content.ComponentName
import android.content.Context
import android.os.Bundle
import android.provider.Settings
import com.bridgething.companion.AndroidMediaSessionGateway
import com.bridgething.companion.AndroidNotificationBackend
import com.bridgething.companion.AndroidPhoneBackend
import com.bridgething.companion.BridgethingCompanion
import com.bridgething.companion.CompanionCapabilityFlags
import com.bridgething.companion.HostInfo
import com.bridgething.gateway.BluetoothSocketAdapter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Process-wide owner of the one live [BridgethingCompanion]. The foreground
 * connection service and the RN session module both reach it through here, so
 * the bluetooth link survives the UI being swiped away. Built once per process;
 * never torn down on device-disappear (idles with no peer instead).
 */
public object CompanionHolder {
    private val mutex = Mutex()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    @Volatile
    public var companion: BridgethingCompanion? = null
        private set

    @Volatile
    public var adapter: BluetoothSocketAdapter? = null
        private set

    @Volatile
    public var foreground: Boolean = false
        private set

    private var lifecycleRegistered = false
    private var startedActivities = 0

    public suspend fun ensureStarted(context: Context): BridgethingCompanion = mutex.withLock {
        (context.applicationContext as? Application)?.let { ensureLifecycleObserver(it) }
        companion?.let { return it }
        val appCtx = context.applicationContext
        val transport = BluetoothSocketAdapter()
        val notificationBackend = AndroidNotificationBackend(
            activeShade = { NotificationBridgeRegistry.listener?.activeWireNotifications() ?: emptyList() },
            resolveAction = { id, positive -> NotificationBridgeRegistry.listener?.actionIntent(id, positive) },
        )
        val c = BridgethingCompanion(
            context = appCtx,
            adapter = transport,
            lyricsResolver = HybridBridgethingSessionImpl.lyricsResolver,
            host = makeHostInfo(appCtx),
            capabilities = CompanionCapabilityFlags(),
            notifications = notificationBackend,
            phone = AndroidPhoneBackend(appCtx),
            mediaSessions = AndroidMediaSessionGateway(
                appCtx,
                ComponentName(appCtx, BridgethingNotificationListener::class.java),
            ),
        )
        c.start()
        companion = c
        adapter = transport
        NotificationBridgeRegistry.companion = c
        NotificationBridgeRegistry.backend = notificationBackend
        reconnectAssociated(appCtx)
        c
    }

    public fun reconnectAssociated(context: Context) {
        val transport = adapter ?: return
        val ba = (context.applicationContext.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter
            ?: return
        for (mac in CompanionDevicePicker.associations(context.applicationContext)) {
            val device = runCatching { ba.getRemoteDevice(mac) }.getOrNull() ?: continue
            scope.launch { runCatching { transport.connect(device) } }
        }
    }

    private fun ensureLifecycleObserver(app: Application) {
        if (lifecycleRegistered) return
        lifecycleRegistered = true
        app.registerActivityLifecycleCallbacks(object : Application.ActivityLifecycleCallbacks {
            override fun onActivityStarted(activity: Activity) {
                startedActivities++
                foreground = true
            }

            override fun onActivityStopped(activity: Activity) {
                startedActivities = (startedActivities - 1).coerceAtLeast(0)
                if (startedActivities == 0) foreground = false
            }

            override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {}
            override fun onActivityResumed(activity: Activity) {}
            override fun onActivityPaused(activity: Activity) {}
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
            override fun onActivityDestroyed(activity: Activity) {}
        })
    }

    @Suppress("HardwareIds")
    internal fun makeHostInfo(context: Context): HostInfo = HostInfo(
        appName = HybridBridgethingSessionImpl.hostInfo.appName,
        appVersion = HybridBridgethingSessionImpl.hostInfo.appVersion,
        osName = "Android",
        osVersion = android.os.Build.VERSION.RELEASE ?: "",
        address = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID) ?: "",
        adapterVersion = "rfcomm",
    )
}
