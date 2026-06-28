package com.margelo.nitro.bridgething.session

import com.bridgething.session.BridgethingSessionBackend
import com.facebook.proguard.annotations.DoNotStrip
import com.margelo.nitro.core.NullType
import com.margelo.nitro.core.Promise

/** thin Nitro proxy; buffers callback setters until a backend is installed via [installBackend]. */
@DoNotStrip
public class HybridBridgethingSession : HybridBridgethingSessionSpec() {

    public companion object {
        private val stateLock = Any()

        @Volatile
        private var backend: BridgethingSessionBackend? = null

        private var pendingProviderChanged: ((BridgethingProviderInfo?) -> Unit)? = null
        private var pendingAuthStateChanged: ((BridgethingAuthState) -> Unit)? = null
        private var pendingServiceHealthChanged: ((BridgethingServiceHealth) -> Unit)? = null
        private var pendingPeerConnected: ((BridgethingSessionPeer) -> Unit)? = null
        private var pendingPeerDisconnected: ((String) -> Unit)? = null
        private var pendingPeerLinkFailed: ((BridgethingSessionPeer) -> Unit)? = null
        private var pendingNowPlayingChanged: ((BridgethingNowPlaying?) -> Unit)? = null
        private var pendingAncsAuthStatusChanged: ((BridgethingAncsAuthStatus) -> Unit)? = null
        private var pendingLog: ((String, String) -> Unit)? = null
        private var pendingWebappsChanged: ((String) -> Unit)? = null
        private var pendingDeviceMetaChanged: ((String, BridgethingDeviceMeta) -> Unit)? = null
        private var pendingOtaEvent: ((BridgethingOtaEvent) -> Unit)? = null
        private var pendingCatalogEvent: ((BridgethingCatalogEvent) -> Unit)? = null

        /** explicit init because bun workspace symlinks trip RN's autolinker. idempotent. */
        @JvmStatic
        public fun initializeNitro() {
            BridgethingSessionOnLoad.initializeNative()
        }

        /** install the backend; must be called before RN starts. replays any already-registered callback setters. */
        @JvmStatic
        public fun installBackend(b: BridgethingSessionBackend) {
            val replay = synchronized(stateLock) {
                backend = b
                val snapshot = Replay(
                    provider = pendingProviderChanged,
                    auth = pendingAuthStateChanged,
                    serviceHealth = pendingServiceHealthChanged,
                    peerConnected = pendingPeerConnected,
                    peerDisconnected = pendingPeerDisconnected,
                    peerLinkFailed = pendingPeerLinkFailed,
                    nowPlaying = pendingNowPlayingChanged,
                    ancs = pendingAncsAuthStatusChanged,
                    log = pendingLog,
                    webapps = pendingWebappsChanged,
                    deviceMeta = pendingDeviceMetaChanged,
                    ota = pendingOtaEvent,
                    catalog = pendingCatalogEvent,
                )
                pendingProviderChanged = null
                pendingAuthStateChanged = null
                pendingServiceHealthChanged = null
                pendingPeerConnected = null
                pendingPeerDisconnected = null
                pendingPeerLinkFailed = null
                pendingNowPlayingChanged = null
                pendingAncsAuthStatusChanged = null
                pendingLog = null
                pendingWebappsChanged = null
                pendingDeviceMetaChanged = null
                pendingOtaEvent = null
                pendingCatalogEvent = null
                snapshot
            }
            replay.provider?.let(b::setOnProviderChanged)
            replay.auth?.let(b::setOnAuthStateChanged)
            replay.serviceHealth?.let(b::setOnServiceHealthChanged)
            replay.peerConnected?.let(b::setOnPeerConnected)
            replay.peerDisconnected?.let(b::setOnPeerDisconnected)
            replay.peerLinkFailed?.let(b::setOnPeerLinkFailed)
            replay.nowPlaying?.let(b::setOnNowPlayingChanged)
            replay.ancs?.let(b::setOnAncsAuthStatusChanged)
            replay.log?.let(b::setOnLog)
            replay.webapps?.let(b::setOnWebappsChanged)
            replay.deviceMeta?.let(b::setOnDeviceMetaChanged)
            replay.ota?.let(b::setOnOtaEvent)
            replay.catalog?.let(b::setOnCatalogEvent)
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
        val serviceHealth: ((BridgethingServiceHealth) -> Unit)?,
        val peerConnected: ((BridgethingSessionPeer) -> Unit)?,
        val peerDisconnected: ((String) -> Unit)?,
        val peerLinkFailed: ((BridgethingSessionPeer) -> Unit)?,
        val nowPlaying: ((BridgethingNowPlaying?) -> Unit)?,
        val ancs: ((BridgethingAncsAuthStatus) -> Unit)?,
        val log: ((String, String) -> Unit)?,
        val webapps: ((String) -> Unit)?,
        val deviceMeta: ((String, BridgethingDeviceMeta) -> Unit)?,
        val ota: ((BridgethingOtaEvent) -> Unit)?,
        val catalog: ((BridgethingCatalogEvent) -> Unit)?,
    )

    override fun start(): Promise<Unit> = Promise.async { require().start() }
    override fun stop(): Promise<Unit> = Promise.async { require().stop() }

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

    override fun snapshot(): Promise<BridgethingSessionSnapshot> = Promise.async {
        require().snapshot()
    }

    override fun deviceLogSnapshot(limit: Double): Promise<Array<BridgethingDeviceLogLine>> = Promise.async {
        backend?.deviceLogSnapshot(limit) ?: emptyArray()
    }

    override fun companionDebug(): Promise<BridgethingCompanionDebug> = Promise.async {
        require().companionDebug()
    }

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

    override fun listWebapps(deviceId: String): Promise<Array<BridgethingWebappInfo>> = Promise.async {
        require().listWebapps(deviceId)
    }

    override fun currentWebapp(deviceId: String): Promise<Variant_NullType_BridgethingActiveWebapp> = Promise.async {
        val active = require().currentWebapp(deviceId)
        if (active != null) Variant_NullType_BridgethingActiveWebapp.Second(active)
        else Variant_NullType_BridgethingActiveWebapp.First(NullType.NULL)
    }

    override fun installWebapp(deviceId: String, sourceUri: String): Promise<BridgethingWebappInfo> = Promise.async {
        require().installWebapp(deviceId, sourceUri)
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

    override fun setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<Unit> = Promise.async {
        backend?.setCapabilityFlags(flags)
    }

    override fun setOtaPollConfig(config: Variant_NullType_BridgethingOtaPollConfig?): Promise<Unit> = Promise.async {
        val unwrapped: BridgethingOtaPollConfig? = config?.let { variant ->
            when (variant) {
                is Variant_NullType_BridgethingOtaPollConfig.First -> null
                is Variant_NullType_BridgethingOtaPollConfig.Second -> variant.value
            }
        }
        backend?.setOtaPollConfig(unwrapped)
    }

    override fun checkForOtaUpdate(channel: String, rootUrl: Variant_NullType_String?): Promise<Unit> = Promise.async {
        backend?.checkForOtaUpdate(channel, unwrapString(rootUrl))
    }

    override fun fetchOtaManifest(rootUrl: Variant_NullType_String?): Promise<BridgethingOtaManifest> = Promise.async {
        require().fetchOtaManifest(unwrapString(rootUrl))
    }

    override fun applyOtaUpdate(deviceId: String, channel: String, version: String, rootUrl: Variant_NullType_String?): Promise<Unit> = Promise.async {
        backend?.applyOtaUpdate(deviceId, channel, version, unwrapString(rootUrl))
    }

    override fun catalogSources(): Promise<Array<String>> = Promise.async {
        require().catalogSources()
    }

    override fun addCatalogSource(url: String): Promise<Unit> = Promise.async {
        backend?.addCatalogSource(url)
    }

    override fun removeCatalogSource(url: String): Promise<Unit> = Promise.async {
        backend?.removeCatalogSource(url)
    }

    override fun refreshCatalog(): Promise<Unit> = Promise.async {
        backend?.refreshCatalog()
    }

    override fun availableCatalogApps(deviceId: String): Promise<String> = Promise.async {
        require().availableCatalogApps(deviceId)
    }

    override fun checkForCatalogUpdates(deviceId: String): Promise<String> = Promise.async {
        require().checkForCatalogUpdates(deviceId)
    }

    override fun installCatalogApp(deviceId: String, appId: String, version: String, sourceUrl: String): Promise<BridgethingWebappInfo> = Promise.async {
        require().installCatalogApp(deviceId, appId, version, sourceUrl)
    }

    override fun setCatalogPollConfig(config: Variant_NullType_BridgethingCatalogPollConfig?): Promise<Unit> = Promise.async {
        val unwrapped: BridgethingCatalogPollConfig? = config?.let { variant ->
            when (variant) {
                is Variant_NullType_BridgethingCatalogPollConfig.First -> null
                is Variant_NullType_BridgethingCatalogPollConfig.Second -> variant.value
            }
        }
        backend?.setCatalogPollConfig(unwrapped)
    }

    override fun reconnectPeer(deviceId: String): Promise<Unit> = Promise.async {
        backend?.reconnectPeer(deviceId)
    }

    override fun presentPairPicker(): Promise<Variant_NullType_BridgethingBtDevice> = Promise.async {
        val device = require().presentPairPicker()
        if (device != null) Variant_NullType_BridgethingBtDevice.Second(device)
        else Variant_NullType_BridgethingBtDevice.First(NullType.NULL)
    }

    override fun isNotificationAccessGranted(): Promise<Boolean> = Promise.async {
        backend?.isNotificationAccessGranted() ?: false
    }

    override fun requestNotificationAccess(): Promise<Unit> = Promise.async {
        require().requestNotificationAccess()
    }

    override fun isDefaultDialer(): Promise<Boolean> = Promise.async {
        backend?.isDefaultDialer() ?: false
    }

    override fun requestDefaultDialer(): Promise<Unit> = Promise.async {
        require().requestDefaultDialer()
    }

    override fun forgetCompanionDevice(mac: String): Promise<Unit> = Promise.async {
        require().forgetCompanionDevice(mac)
    }

    override fun isIgnoringBatteryOptimizations(): Promise<Boolean> = Promise.async {
        backend?.isIgnoringBatteryOptimizations() ?: false
    }

    override fun requestIgnoreBatteryOptimizations(): Promise<Unit> = Promise.async {
        require().requestIgnoreBatteryOptimizations()
    }

    override fun revokeRuntimePermissions(permissions: Array<String>): Promise<Boolean> = Promise.async {
        backend?.revokeRuntimePermissions(permissions) ?: false
    }

    override fun killApp(): Promise<Unit> = Promise.async {
        backend?.killApp()
    }

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

    override fun setOnServiceHealthChanged(callback: (health: BridgethingServiceHealth) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnServiceHealthChanged) { pendingServiceHealthChanged = it }
    }

    override fun setOnPeerConnected(callback: (peer: BridgethingSessionPeer) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnPeerConnected) { pendingPeerConnected = it }
    }

    override fun setOnPeerDisconnected(callback: (peerId: String) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnPeerDisconnected) { pendingPeerDisconnected = it }
    }

    override fun setOnPeerLinkFailed(callback: (peer: BridgethingSessionPeer) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnPeerLinkFailed) { pendingPeerLinkFailed = it }
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

    override fun setLocalLogStreamingEnabled(enabled: Boolean) {
        backend?.setLocalLogStreamingEnabled(enabled)
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

    override fun setOnCatalogEvent(callback: (event: BridgethingCatalogEvent) -> Unit) {
        forwardOrBuffer(callback, BridgethingSessionBackend::setOnCatalogEvent) { pendingCatalogEvent = it }
    }

    private fun unwrapString(variant: Variant_NullType_String?): String? = variant?.let {
        when (it) {
            is Variant_NullType_String.First -> null
            is Variant_NullType_String.Second -> it.value
        }
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
