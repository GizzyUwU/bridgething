package com.bridgething

import android.bluetooth.BluetoothManager
import android.content.Context
import android.provider.Settings
import dev.bridgething.companion.AndroidNotificationActionBackend
import dev.bridgething.companion.AndroidPhoneBackend
import dev.bridgething.companion.BridgethingCompanion
import dev.bridgething.companion.CompanionCapabilityFlags
import dev.bridgething.companion.HostInfo
import dev.bridgething.gateway.BluetoothSocketAdapter
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

    public suspend fun ensureStarted(context: Context): BridgethingCompanion = mutex.withLock {
        companion?.let { return it }
        val appCtx = context.applicationContext
        val transport = BluetoothSocketAdapter()
        val c = BridgethingCompanion(
            context = appCtx,
            adapter = transport,
            lyricsResolver = HybridBridgethingSessionImpl.lyricsResolver,
            host = makeHostInfo(appCtx),
            capabilities = CompanionCapabilityFlags(),
            notificationActions = AndroidNotificationActionBackend { id, positive ->
                NotificationBridgeRegistry.listener?.actionIntent(id, positive)
            },
            phone = AndroidPhoneBackend(appCtx),
        )
        c.start()
        companion = c
        adapter = transport
        NotificationBridgeRegistry.companion = c
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

    public suspend fun shutdown() {
        val prior = mutex.withLock {
            val c = companion
            companion = null
            adapter = null
            NotificationBridgeRegistry.companion = null
            c
        }
        prior?.stop()
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
