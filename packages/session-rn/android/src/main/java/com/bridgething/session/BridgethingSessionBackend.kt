package com.bridgething.session

import com.margelo.nitro.bridgething.session.BridgethingActiveWebapp
import com.margelo.nitro.bridgething.session.BridgethingAncsAuthStatus
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupResult
import com.margelo.nitro.bridgething.session.BridgethingAuthState
import com.margelo.nitro.bridgething.session.BridgethingBtDevice
import com.margelo.nitro.bridgething.session.BridgethingCapabilityFlags
import com.margelo.nitro.bridgething.session.BridgethingConfigEntry
import com.margelo.nitro.bridgething.session.BridgethingDeviceMeta
import com.margelo.nitro.bridgething.session.BridgethingHostInfo
import com.margelo.nitro.bridgething.session.BridgethingNowPlaying
import com.margelo.nitro.bridgething.session.BridgethingOtaEvent
import com.margelo.nitro.bridgething.session.BridgethingOtaPollConfig
import com.margelo.nitro.bridgething.session.BridgethingProviderInfo
import com.margelo.nitro.bridgething.session.BridgethingSessionPeer
import com.margelo.nitro.bridgething.session.BridgethingWebappIcon
import com.margelo.nitro.bridgething.session.BridgethingWebappInfo

/**
 * Backend protocol the host app implements. Decouples the Nitro
 * HybridObject (lives in this library, can't see gradle subprojects
 * outside its scope) from the orchestration logic (lives in the host
 * app target where the bridgething companion + glue packages are linked).
 *
 * The host app implements [BridgethingSessionBackend] and registers it
 * with [HybridBridgethingSession.installBackend] at app launch (before
 * any JS code runs). Mirror of the Swift `BridgethingSessionBackend`
 * protocol; same shape so the RN session API behaves identically across
 * platforms.
 */
public interface BridgethingSessionBackend {
    public suspend fun start()
    public suspend fun stop()

    public suspend fun availableProviders(): Array<BridgethingProviderInfo>
    public suspend fun setActiveProvider(id: String?)
    public suspend fun cancelAuth()
    public suspend fun signOut()
    public suspend fun currentProvider(): BridgethingProviderInfo?
    public suspend fun connectedPeers(): Array<BridgethingSessionPeer>
    public suspend fun currentNowPlaying(): BridgethingNowPlaying?

    public suspend fun enableAncsNotifications(): BridgethingAncsSetupResult
    public suspend fun ancsAuthStatus(): BridgethingAncsAuthStatus

    // Webapps (per-device)
    public suspend fun listWebapps(deviceId: String): Array<BridgethingWebappInfo>
    public suspend fun currentWebapp(deviceId: String): BridgethingActiveWebapp?
    public suspend fun installWebappFromBase64(deviceId: String, archiveBase64: String): BridgethingWebappInfo
    public suspend fun uninstallWebapp(deviceId: String, id: String)
    public suspend fun switchWebapp(deviceId: String, id: String)
    public suspend fun webappIcon(deviceId: String, id: String): BridgethingWebappIcon?
    public suspend fun listWebappConfig(deviceId: String, id: String): Array<BridgethingConfigEntry>
    public suspend fun setWebappConfigField(deviceId: String, id: String, key: String, value: String)
    public suspend fun deleteWebappConfigField(deviceId: String, id: String, key: String)

    // Capability flags
    public suspend fun setCapabilityFlags(flags: BridgethingCapabilityFlags)

    // OTA
    public suspend fun setOtaPollConfig(config: BridgethingOtaPollConfig?)
    public suspend fun pollOtaNow()
    public suspend fun deviceMeta(deviceId: String): BridgethingDeviceMeta?

    // Host identity
    public suspend fun hostInfo(): BridgethingHostInfo

    // OS-mediated pair flow (iOS = ASK, android = CompanionDeviceManager)
    public suspend fun presentPairPicker(): BridgethingBtDevice?

    // Notification access (android only)
    public suspend fun isNotificationAccessGranted(): Boolean
    public suspend fun requestNotificationAccess()

    public suspend fun revokeRuntimePermissions(permissions: Array<String>): Boolean
    public suspend fun killApp()

    public fun setOnProviderChanged(callback: (BridgethingProviderInfo?) -> Unit)
    public fun setOnAuthStateChanged(callback: (BridgethingAuthState) -> Unit)
    public fun setOnPeerConnected(callback: (BridgethingSessionPeer) -> Unit)
    public fun setOnPeerDisconnected(callback: (String) -> Unit)
    public fun setOnNowPlayingChanged(callback: (BridgethingNowPlaying?) -> Unit)
    public fun setOnAncsAuthStatusChanged(callback: (BridgethingAncsAuthStatus) -> Unit)
    public fun setOnLog(callback: (String, String) -> Unit)
    public fun setLogStreamingEnabled(enabled: Boolean)
    public fun setOnWebappsChanged(callback: (String) -> Unit)
    public fun setOnDeviceMetaChanged(callback: (String, BridgethingDeviceMeta) -> Unit)
    public fun setOnOtaEvent(callback: (BridgethingOtaEvent) -> Unit)
}
