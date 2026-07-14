package com.bridgething

import android.app.Activity
import android.app.Application
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.ComponentName
import android.util.Log
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
    private const val TAG = "BridgethingBT"
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
        BondWatcher.register(appCtx)
        val transport = BluetoothSocketAdapter(
            bluetooth = (appCtx.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter,
        )
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

    /**
     * Reconnect every associated device that is actually bonded.
     *
     * Unbonded associations are skipped rather than connected: a CDM association
     * outlives its BT bond, and connecting RFCOMM to an unbonded peer is how
     * Android starts pairing - which is what made reconnects re-pair in the
     * background. If such a device ever bonds again, [BondWatcher] connects it.
     *
     * Safe to call concurrently and repeatedly (the foreground service restarts,
     * CDM presence wakeups and the pair flow all land here); the adapter folds
     * concurrent connects for one device into a single attempt.
     */
    public fun reconnectAssociated(context: Context) {
        val transport = adapter
        if (transport == null) {
            Log.w(TAG, "reconnectAssociated: no adapter yet")
            return
        }
        val ba = (context.applicationContext.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager)?.adapter
        if (ba == null) {
            Log.w(TAG, "reconnectAssociated: bluetooth adapter unavailable")
            return
        }
        val macs = CompanionDevicePicker.associations(context.applicationContext)
        Log.i(TAG, "reconnectAssociated: ${macs.size} association(s) $macs")
        for (mac in macs) {
            // CDM hands back lowercase MACs (MacAddress.toString()), but
            // BluetoothAdapter.getRemoteDevice rejects anything but uppercase
            // with IllegalArgumentException. Normalize or the connect silently
            // never happens.
            val device = runCatching { ba.getRemoteDevice(mac.uppercase()) }.getOrNull()
            if (device == null) {
                Log.w(TAG, "reconnectAssociated: getRemoteDevice($mac) failed")
                continue
            }
            val bond = runCatching { device.bondState }.getOrDefault(BluetoothDevice.BOND_NONE)
            if (bond != BluetoothDevice.BOND_BONDED) {
                Log.i(TAG, "reconnectAssociated: skipping $mac, not bonded (bondState=$bond)")
                continue
            }
            Log.i(TAG, "reconnectAssociated: connecting $mac")
            scope.launch {
                runCatching { transport.connect(device) }
                    .onFailure { Log.w(TAG, "reconnectAssociated: connect($mac) failed: ${it.message}") }
            }
        }
    }

    /** Bring up a peer that just reached BOND_BONDED. Called by [BondWatcher]. */
    public suspend fun connectBonded(context: Context, device: BluetoothDevice) {
        if (adapter == null) runCatching { ensureStarted(context) }
        val transport = adapter ?: run {
            Log.w(TAG, "connectBonded: no adapter for ${device.address}")
            return
        }
        Log.i(TAG, "connectBonded: ${device.address} bonded, connecting")
        runCatching { transport.connect(device) }
            .onFailure { Log.w(TAG, "connectBonded: connect(${device.address}) failed: ${it.message}") }
    }

    /** Stop tracking a peer that lost its bond, so nothing reconnects (or re-pairs) to it. */
    public suspend fun forgetDevice(mac: String) {
        val transport = adapter ?: return
        runCatching { transport.forget(mac.uppercase()) }
            .onFailure { Log.w(TAG, "forgetDevice($mac) failed: ${it.message}") }
    }

    private fun ensureLifecycleObserver(app: Application) {
        if (lifecycleRegistered) return
        lifecycleRegistered = true
        // The companion is usually created while the activity is already
        // resumed, so the first onActivityStarted fired before we registered
        // and would be missed - leaving `foreground` stuck false until the next
        // app switch, which silently drops every foreground-gated event (auth,
        // peer, catalog, ...) on first launch. Seed from the current activity.
        if (BridgethingActivityRegistry.currentActivity != null) {
            startedActivities = 1
            foreground = true
        }
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
