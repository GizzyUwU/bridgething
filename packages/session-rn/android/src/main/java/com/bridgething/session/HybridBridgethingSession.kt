package com.bridgething.session

import com.facebook.proguard.annotations.DoNotStrip
import com.margelo.nitro.bridgething.session.BridgethingActiveWebapp
import com.margelo.nitro.bridgething.session.BridgethingAncsAuthStatus
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupKind
import com.margelo.nitro.bridgething.session.BridgethingAncsSetupResult
import com.margelo.nitro.bridgething.session.BridgethingAuthState
import com.margelo.nitro.bridgething.session.BridgethingCapabilityFlags
import com.margelo.nitro.bridgething.session.BridgethingConfigEntry
import com.margelo.nitro.bridgething.session.BridgethingDeviceMeta
import com.margelo.nitro.bridgething.session.BridgethingHostInfo
import com.margelo.nitro.bridgething.session.BridgethingOtaEvent
import com.margelo.nitro.bridgething.session.BridgethingProviderInfo
import com.margelo.nitro.bridgething.session.BridgethingSessionPeer
import com.margelo.nitro.bridgething.session.BridgethingWebappInfo
import com.margelo.nitro.bridgething.session.HybridBridgethingSessionSpec
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingActiveWebapp
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingDeviceMeta
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingNowPlaying
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingOtaPollConfig
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingProviderInfo
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingWebappIcon
import com.margelo.nitro.bridgething.session.Variant_NullType_String
import com.margelo.nitro.core.NullType
import com.margelo.nitro.core.Promise

/**
 * Android-side bridgething session module. Mirrors `HybridBridgethingSession`
 * on the iOS side: owns one `BridgethingCompanion` (`packages/companion/kotlin`),
 * which owns the gateway, the active glue, and every dispatcher.
 *
 * Implementation lands in a follow-up Android slice - the surface is here
 * so the iOS impl, the RN session SDK, and `mobile/App.tsx` can compile
 * end-to-end.
 */
@DoNotStrip
class HybridBridgethingSession : HybridBridgethingSessionSpec() {
    override fun start(): Promise<Unit> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun stop(): Promise<Unit> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun availableProviders(): Promise<Array<BridgethingProviderInfo>> = Promise.async {
        emptyArray()
    }

    override fun setActiveProvider(id: Variant_NullType_String?): Promise<Unit> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun currentProvider(): Promise<Variant_NullType_BridgethingProviderInfo> = Promise.async {
        Variant_NullType_BridgethingProviderInfo.First(NullType.NULL)
    }

    override fun cancelAuth(): Promise<Unit> = Promise.async {
        // No active auth on the stub.
    }

    override fun signOut(): Promise<Unit> = Promise.async {
        // No persisted credentials on the stub.
    }

    override fun connectedPeers(): Promise<Array<BridgethingSessionPeer>> = Promise.async {
        emptyArray()
    }

    override fun currentNowPlaying(): Promise<Variant_NullType_BridgethingNowPlaying> = Promise.async {
        Variant_NullType_BridgethingNowPlaying.First(NullType.NULL)
    }

    override fun enableAncsNotifications(): Promise<BridgethingAncsSetupResult> = Promise.async {
        BridgethingAncsSetupResult(
            kind = BridgethingAncsSetupKind.UNSUPPORTED,
            authStatus = BridgethingAncsAuthStatus.UNKNOWN,
            message = null,
        )
    }

    override fun ancsAuthStatus(): Promise<BridgethingAncsAuthStatus> = Promise.async {
        BridgethingAncsAuthStatus.UNKNOWN
    }

    override fun listWebapps(deviceId: String): Promise<Array<BridgethingWebappInfo>> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun currentWebapp(deviceId: String): Promise<Variant_NullType_BridgethingActiveWebapp> = Promise.async {
        Variant_NullType_BridgethingActiveWebapp.First(NullType.NULL)
    }

    override fun installWebappFromBase64(deviceId: String, archiveBase64: String): Promise<BridgethingWebappInfo> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun uninstallWebapp(deviceId: String, id: String): Promise<Unit> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun switchWebapp(deviceId: String, id: String): Promise<Unit> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun webappIcon(deviceId: String, id: String): Promise<Variant_NullType_BridgethingWebappIcon> = Promise.async {
        Variant_NullType_BridgethingWebappIcon.First(NullType.NULL)
    }

    override fun listWebappConfig(deviceId: String, id: String): Promise<Array<BridgethingConfigEntry>> = Promise.async {
        emptyArray()
    }

    override fun setWebappConfigField(deviceId: String, id: String, key: String, value: String): Promise<Unit> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun deleteWebappConfigField(deviceId: String, id: String, key: String): Promise<Unit> = Promise.async {
        throw RuntimeException("Android implementation pending")
    }

    override fun setCapabilityFlags(flags: BridgethingCapabilityFlags): Promise<Unit> = Promise.async {
        // Stub: no companion to forward to yet.
    }

    override fun setOtaPollConfig(config: Variant_NullType_BridgethingOtaPollConfig?): Promise<Unit> = Promise.async {
        // Stub: no OTA service yet.
    }

    override fun pollOtaNow(): Promise<Unit> = Promise.async {
        // Stub: no OTA service yet.
    }

    override fun deviceMeta(deviceId: String): Promise<Variant_NullType_BridgethingDeviceMeta> = Promise.async {
        Variant_NullType_BridgethingDeviceMeta.First(NullType.NULL)
    }

    override fun hostInfo(): Promise<BridgethingHostInfo> = Promise.async {
        BridgethingHostInfo(
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

    override fun setOnProviderChanged(callback: (info: Variant_NullType_BridgethingProviderInfo?) -> Unit) {}
    override fun setOnAuthStateChanged(callback: (state: BridgethingAuthState) -> Unit) {}
    override fun setOnPeerConnected(callback: (peer: BridgethingSessionPeer) -> Unit) {}
    override fun setOnPeerDisconnected(callback: (peerId: String) -> Unit) {}
    override fun setOnNowPlayingChanged(callback: (now: Variant_NullType_BridgethingNowPlaying?) -> Unit) {}
    override fun setOnAncsAuthStatusChanged(callback: (status: BridgethingAncsAuthStatus) -> Unit) {}
    override fun setOnLog(callback: (level: String, message: String) -> Unit) {}
    override fun setLogStreamingEnabled(enabled: Boolean) {}
    override fun setOnWebappsChanged(callback: (deviceId: String) -> Unit) {}
    override fun setOnDeviceMetaChanged(callback: (deviceId: String, meta: BridgethingDeviceMeta) -> Unit) {}
    override fun setOnOtaEvent(callback: (event: BridgethingOtaEvent) -> Unit) {}
}
