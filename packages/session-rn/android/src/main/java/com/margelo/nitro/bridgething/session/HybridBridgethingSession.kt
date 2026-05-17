package com.margelo.nitro.bridgething.session

import com.bridgething.session.BridgethingSessionBackend
import com.facebook.proguard.annotations.DoNotStrip
import com.margelo.nitro.core.NullType
import com.margelo.nitro.core.Promise

/**
 * Thin Nitro proxy. The library ships this; the host app installs a
 * [BridgethingSessionBackend] at launch via [installBackend]. Without a
 * backend installed, every method throws "backend not installed".
 * Callback setters are silently buffered until a backend is installed,
 * then re-applied. Mirror of the Swift `HybridBridgethingSession` proxy.
 */
@DoNotStrip
public class HybridBridgethingSession : HybridBridgethingSessionSpec() {

    public companion object {
        private val stateLock = Any()

        @Volatile
        private var backend: BridgethingSessionBackend? = null

        private var pendingProviderChanged: ((BridgethingProviderInfo?) -> Unit)? = null
        private var pendingAuthStateChanged: ((BridgethingAuthState) -> Unit)? = null
        private var pendingPeerConnected: ((BridgethingSessionPeer) -> Unit)? = null
        private var pendingPeerDisconnected: ((String) -> Unit)? = null
        private var pendingNowPlayingChanged: ((BridgethingNowPlaying?) -> Unit)? = null
        private var pendingAncsAuthStatusChanged: ((BridgethingAncsAuthStatus) -> Unit)? = null
        private var pendingLog: ((String, String) -> Unit)? = null
        private var pendingWebappsChanged: ((String) -> Unit)? = null
        private var pendingDeviceMetaChanged: ((String, BridgethingDeviceMeta) -> Unit)? = null
        private var pendingOtaEvent: ((BridgethingOtaEvent) -> Unit)? = null
        private var pendingBtDiscoveryEvent: ((BridgethingBtDiscoveryEvent) -> Unit)? = null
        private var pendingBtBondStateChanged: ((BridgethingBtDevice) -> Unit)? = null

        /**
         * Load the BridgethingSession JNI lib + register the HybridObject
         * with Nitro's runtime registry. Normally autolinking generates
         * this call into the app's OnLoad.cpp, but bun workspace symlinks
         * trip RN's autolinker so the host app calls this explicitly from
         * MainApplication.onCreate. Idempotent; safe to call more than once.
         */
        @JvmStatic
        public fun initializeNitro() {
            BridgethingSessionOnLoad.initializeNative()
        }

        /**
         * Host apps call this once at launch (before React Native starts)
         * to wire up the real session backend that uses the bridgething
         * Kotlin packages. Replays any callback setters JS may have
         * already installed.
         */
        @JvmStatic
        public fun installBackend(b: BridgethingSessionBackend) {
            val replay = synchronized(stateLock) {
                backend = b
                val snapshot = Replay(
                    provider = pendingProviderChanged,
                    auth = pendingAuthStateChanged,
                    peerConnected = pendingPeerConnected,
                    peerDisconnected = pendingPeerDisconnected,
                    nowPlaying = pendingNowPlayingChanged,
                    ancs = pendingAncsAuthStatusChanged,
                    log = pendingLog,
                    webapps = pendingWebappsChanged,
                    deviceMeta = pendingDeviceMetaChanged,
                    ota = pendingOtaEvent,
                    btDiscovery = pendingBtDiscoveryEvent,
                    btBondState = pendingBtBondStateChanged,
                )
                pendingProviderChanged = null
                pendingAuthStateChanged = null
                pendingPeerConnected = null
                pendingPeerDisconnected = null
                pendingNowPlayingChanged = null
                pendingAncsAuthStatusChanged = null
                pendingLog = null
                pendingWebappsChanged = null
                pendingDeviceMetaChanged = null
                pendingOtaEvent = null
                pendingBtDiscoveryEvent = null
                pendingBtBondStateChanged = null
                snapshot
            }
            replay.provider?.let(b::setOnProviderChanged)
            replay.auth?.let(b::setOnAuthStateChanged)
            replay.peerConnected?.let(b::setOnPeerConnected)
            replay.peerDisconnected?.let(b::setOnPeerDisconnected)
            replay.nowPlaying?.let(b::setOnNowPlayingChanged)
            replay.ancs?.let(b::setOnAncsAuthStatusChanged)
            replay.log?.let(b::setOnLog)
            replay.webapps?.let(b::setOnWebappsChanged)
            replay.deviceMeta?.let(b::setOnDeviceMetaChanged)
            replay.ota?.let(b::setOnOtaEvent)
            replay.btDiscovery?.let(b::setOnBluetoothDiscoveryEvent)
            replay.btBondState?.let(b::setOnBluetoothBondStateChanged)
        }

        private fun require(): BridgethingSessionBackend = backend
            ?: throw RuntimeException(
                "BridgethingSession backend not installed - host app must call " +
                    "HybridBridgethingSession.installBackend(...) before React Native starts"
            )
    }

    private data class Replay(
        val provider: ((BridgethingProviderInfo?) -> Unit)?,
        val auth: ((BridgethingAuthState) -> Unit)?,
        val peerConnected: ((BridgethingSessionPeer) -> Unit)?,
        val peerDisconnected: ((String) -> Unit)?,
        val nowPlaying: ((BridgethingNowPlaying?) -> Unit)?,
        val ancs: ((BridgethingAncsAuthStatus) -> Unit)?,
        val log: ((String, String) -> Unit)?,
        val webapps: ((String) -> Unit)?,
        val deviceMeta: ((String, BridgethingDeviceMeta) -> Unit)?,
        val ota: ((BridgethingOtaEvent) -> Unit)?,
        val btDiscovery: ((BridgethingBtDiscoveryEvent) -> Unit)?,
        val btBondState: ((BridgethingBtDevice) -> Unit)?,
    )

    // Lifecycle

    override fun start(): Promise<Unit> = Promise.async { require().start() }
    override fun stop(): Promise<Unit> = Promise.async { require().stop() }

    // Provider selection

    override fun availableProviders(): Promise<Array<BridgethingProviderInfo>> = Promise.async {
        require().availableProviders()
    }

    override fun setActiveProvider(id: Variant_NullType_String?): Promise<Unit> = Promise.async {
        val unwrapped: String? = id?.let { variant ->
            when (variant) {
                is Variant_NullType_String.First -> null
                is Variant_NullType_String.Second -> variant.value
            }
        }
        require().setActiveProvider(unwrapped)
    }

    override fun currentProvider(): Promise<Variant_NullType_BridgethingProviderInfo> = Promise.async {
        val info = require().currentProvider()
        if (info != null) Variant_NullType_BridgethingProviderInfo.Second(info)
        else Variant_NullType_BridgethingProviderInfo.First(NullType.NULL)
    }

    override fun cancelAuth(): Promise<Unit> = Promise.async { backend?.cancelAuth() }
    override fun signOut(): Promise<Unit> = Promise.async { backend?.signOut() }

    override fun connectedPeers(): Promise<Array<BridgethingSessionPeer>> = Promise.async {
        backend?.connectedPeers() ?: emptyArray()
    }

    override fun currentNowPlaying(): Promise<Variant_NullType_BridgethingNowPlaying> = Promise.async {
        val np = backend?.currentNowPlaying()
        if (np != null) Variant_NullType_BridgethingNowPlaying.Second(np)
        else Variant_NullType_BridgethingNowPlaying.First(NullType.NULL)
    }

    // ANCS

    override fun enableAncsNotifications(): Promise<BridgethingAncsSetupResult> = Promise.async {
        backend?.enableAncsNotifications() ?: BridgethingAncsSetupResult(
            kind = BridgethingAncsSetupKind.UNSUPPORTED,
            authStatus = BridgethingAncsAuthStatus.UNKNOWN,
            message = null,
        )
    }

    override fun ancsAuthStatus(): Promise<BridgethingAncsAuthStatus> = Promise.async {
        backend?.ancsAuthStatus() ?: BridgethingAncsAuthStatus.UNKNOWN
    }

    // Webapps (per-device)

    override fun listWebapps(deviceId: String): Promise<Array<BridgethingWebappInfo>> = Promise.async {
        require().listWebapps(deviceId)
    }

    override fun currentWebapp(deviceId: String): Promise<Variant_NullType_BridgethingActiveWebapp> = Promise.async {
        val active = require().currentWebapp(deviceId)
        if (active != null) Variant_NullType_BridgethingActiveWebapp.Second(active)
        else Variant_NullType_BridgethingActiveWebapp.First(NullType.NULL)
    }

    override fun installWebappFromBase64(deviceId: String, archiveBase64: String): Promise<BridgethingWebappInfo> = Promise.async {
        require().installWebappFromBase64(deviceId, archiveBase64)
    }

    override fun uninstallWebapp(deviceId: String, id: String): Promise<Unit> = Promise.async {
        require().uninstallWebapp(deviceId, id)
    }

    override fun switchWebapp(deviceId: String, id: String): Promise<Unit> = Promise.async {
        require().switchWebapp(deviceId, id)
    }

    override fun webappIcon(deviceId: String, id: String): Promise<Variant_NullType_BridgethingWebappIcon> = Promise.async {
        val icon = require().webappIcon(deviceId, id)
        if (icon != null) Variant_NullType_BridgethingWebappIcon.Second(icon)
        else Variant_NullType_BridgethingWebappIcon.First(NullType.NULL)
    }

    override fun listWebappConfig(deviceId: String, id: String): Promise<Array<BridgethingConfigEntry>> = Promise.async {
        require().listWebappConfig(deviceId, id)
    }

    override fun setWebappConfigField(deviceId: String, id: String, key: String, value: String): Promise<Unit> = Promise.async {
        require().setWebappConfigField(deviceId, id, key, value)
    }

    override fun deleteWebappConfigField(deviceId: String, id: String, key: String): Promise<Unit> = Promise.async {
        require().deleteWebappConfigField(deviceId, id, key)
    }

    // Capability flags

    override fun setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<Unit> = Promise.async {
        backend?.setCapabilityFlags(flags)
    }

    // OTA

    override fun setOtaPollConfig(config: Variant_NullType_BridgethingOtaPollConfig?): Promise<Unit> = Promise.async {
        val unwrapped: BridgethingOtaPollConfig? = config?.let { variant ->
            when (variant) {
                is Variant_NullType_BridgethingOtaPollConfig.First -> null
                is Variant_NullType_BridgethingOtaPollConfig.Second -> variant.value
            }
        }
        backend?.setOtaPollConfig(unwrapped)
    }

    override fun pollOtaNow(): Promise<Unit> = Promise.async { backend?.pollOtaNow() }

    override fun deviceMeta(deviceId: String): Promise<Variant_NullType_BridgethingDeviceMeta> = Promise.async {
        val meta = backend?.deviceMeta(deviceId)
        if (meta != null) Variant_NullType_BridgethingDeviceMeta.Second(meta)
        else Variant_NullType_BridgethingDeviceMeta.First(NullType.NULL)
    }

    // Host identity

    override fun hostInfo(): Promise<BridgethingHostInfo> = Promise.async {
        backend?.hostInfo() ?: BridgethingHostInfo(
            appName = "bridgething",
            appVersion = "0.0.0",
            osName = "Android",
            osVersion = "",
            hostIdentifier = "",
            libVersion = "",
            libbridgethingVersion = "",
            adapterVersion = "rfcomm",
        )
    }

    // In-app Bluetooth pairing

    override fun listBondedBluetoothDevices(): Promise<Array<BridgethingBtDevice>> = Promise.async {
        backend?.listBondedBluetoothDevices() ?: emptyArray()
    }

    override fun startBluetoothDiscovery(): Promise<Unit> = Promise.async {
        require().startBluetoothDiscovery()
    }

    override fun stopBluetoothDiscovery(): Promise<Unit> = Promise.async {
        backend?.stopBluetoothDiscovery()
    }

    override fun pairBluetoothDevice(address: String): Promise<BridgethingBtBondState> = Promise.async {
        require().pairBluetoothDevice(address)
    }

    override fun presentPairPicker(): Promise<Variant_NullType_BridgethingBtDevice> = Promise.async {
        val device = require().presentPairPicker()
        if (device != null) Variant_NullType_BridgethingBtDevice.Second(device)
        else Variant_NullType_BridgethingBtDevice.First(NullType.NULL)
    }

    // Notification access

    override fun isNotificationAccessGranted(): Promise<Boolean> = Promise.async {
        backend?.isNotificationAccessGranted() ?: false
    }

    override fun requestNotificationAccess(): Promise<Unit> = Promise.async {
        require().requestNotificationAccess()
    }

    override fun revokeRuntimePermissions(permissions: Array<String>): Promise<Boolean> = Promise.async {
        backend?.revokeRuntimePermissions(permissions) ?: false
    }

    override fun killApp(): Promise<Unit> = Promise.async {
        backend?.killApp()
    }

    // Callback setters

    override fun setOnProviderChanged(callback: (info: Variant_NullType_BridgethingProviderInfo?) -> Unit) {
        val wrapped: (BridgethingProviderInfo?) -> Unit = { info ->
            val variant = if (info != null) Variant_NullType_BridgethingProviderInfo.Second(info)
            else Variant_NullType_BridgethingProviderInfo.First(NullType.NULL)
            callback(variant)
        }
        forwardOrBuffer(wrapped, BridgethingSessionBackend::setOnProviderChanged) { pendingProviderChanged = it }
    }

    override fun setOnAuthStateChanged(callback: (state: BridgethingAuthState) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnAuthStateChanged) { pendingAuthStateChanged = it }
    }

    override fun setOnPeerConnected(callback: (peer: BridgethingSessionPeer) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnPeerConnected) { pendingPeerConnected = it }
    }

    override fun setOnPeerDisconnected(callback: (peerId: String) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnPeerDisconnected) { pendingPeerDisconnected = it }
    }

    override fun setOnNowPlayingChanged(callback: (now: Variant_NullType_BridgethingNowPlaying?) -> Unit) {
        val wrapped: (BridgethingNowPlaying?) -> Unit = { np ->
            val variant = if (np != null) Variant_NullType_BridgethingNowPlaying.Second(np)
            else Variant_NullType_BridgethingNowPlaying.First(NullType.NULL)
            callback(variant)
        }
        forwardOrBuffer(wrapped, BridgethingSessionBackend::setOnNowPlayingChanged) { pendingNowPlayingChanged = it }
    }

    override fun setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnAncsAuthStatusChanged) { pendingAncsAuthStatusChanged = it }
    }

    override fun setOnLog(callback: (level: String, message: String) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnLog) { pendingLog = it }
    }

    override fun setLogStreamingEnabled(enabled: Boolean) {
        backend?.setLogStreamingEnabled(enabled)
    }

    override fun setOnWebappsChanged(callback: (deviceId: String) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnWebappsChanged) { pendingWebappsChanged = it }
    }

    override fun setOnDeviceMetaChanged(callback: (deviceId: String, meta: BridgethingDeviceMeta) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnDeviceMetaChanged) { pendingDeviceMetaChanged = it }
    }

    override fun setOnOtaEvent(callback: (event: BridgethingOtaEvent) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnOtaEvent) { pendingOtaEvent = it }
    }

    override fun setOnBluetoothDiscoveryEvent(callback: (event: BridgethingBtDiscoveryEvent) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnBluetoothDiscoveryEvent) { pendingBtDiscoveryEvent = it }
    }

    override fun setOnBluetoothBondStateChanged(callback: (device: BridgethingBtDevice) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnBluetoothBondStateChanged) { pendingBtBondStateChanged = it }
    }

    private inline fun <C> forwardOrBuffer(
        callback: C,
        forward: BridgethingSessionBackend.(C) -> Unit,
        buffer: (C) -> Unit,
    ) {
        val current = synchronized(stateLock) {
            val b = backend
            if (b == null) buffer(callback)
            b
        }
        if (current != null) current.forward(callback)
    }
}
