package com.bridgething.session

import com.facebook.proguard.annotations.DoNotStrip
import com.margelo.nitro.bridgething.session.BridgethingAuthState
import com.margelo.nitro.bridgething.session.BridgethingProviderInfo
import com.margelo.nitro.bridgething.session.BridgethingSessionPeer
import com.margelo.nitro.bridgething.session.HybridBridgethingSessionSpec
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingNowPlaying
import com.margelo.nitro.bridgething.session.Variant_NullType_BridgethingProviderInfo
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

    override fun setOnProviderChanged(callback: (info: Variant_NullType_BridgethingProviderInfo?) -> Unit) {}
    override fun setOnAuthStateChanged(callback: (state: BridgethingAuthState) -> Unit) {}
    override fun setOnPeerConnected(callback: (peer: BridgethingSessionPeer) -> Unit) {}
    override fun setOnPeerDisconnected(callback: (peerId: String) -> Unit) {}
    override fun setOnNowPlayingChanged(callback: (now: Variant_NullType_BridgethingNowPlaying?) -> Unit) {}
    override fun setOnLog(callback: (level: String, message: String) -> Unit) {}
}
