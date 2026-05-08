package dev.bridgething.companion

import dev.bridgething.gateway.Adapter
import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueNowPlaying
import dev.bridgething.lyrics.LyricsResolver

/**
 * Identity the companion advertises in `GatewayCapabilities.gateway`.
 * Caller-supplied at companion init.
 */
data class HostInfo(
    val appName: String,
    val appVersion: String,
    val osName: String,
)

/**
 * Capability flags the companion declares. Glue contributions
 * (`uriSchemes`, `musicProvider`, `lyricsSupported`) are mixed in by
 * `BridgethingCompanion` at announce time.
 */
data class CompanionCapabilityFlags(
    val geo: Boolean = true,
    val notifications: Boolean = false,
    val netFetch: Boolean = true,
    val netWs: Boolean = true,
    val audioTts: Boolean = false,
)

/**
 * Top-level orchestrator for the bridgething companion app on Android.
 *
 * Mirror of the Swift `BridgethingCompanion` actor. Owns one
 * `BridgethingGateway` over the supplied transport adapter, holds at most
 * one active `BridgethingGlue`, and runs every companion-side dispatcher
 * as long-lived child coroutines while started: Player verbs to glue,
 * Lyrics requests with resolver fallback, Asset requests to glue, Net
 * (fetch/ws/stream) via OkHttp, Geo via FusedLocationProviderClient,
 * Volume via AudioManager.
 *
 * Implementation lands in a follow-up Android slice; the constructor +
 * public method shape is stable from this round so the iOS companion +
 * RN session API can be designed against it.
 */
class BridgethingCompanion(
    adapter: Adapter,
    private val lyricsResolver: LyricsResolver,
    private val host: HostInfo,
    private val capabilities: CompanionCapabilityFlags = CompanionCapabilityFlags(),
) {
    val gateway: BridgethingGateway = BridgethingGateway(adapter)
    public val ota: OtaService = OtaService()

    private var activeGlue: BridgethingGlue? = null

    suspend fun start() {
        TODO("Android implementation pending")
    }

    suspend fun stop() {
        TODO("Android implementation pending")
    }

    suspend fun setActive(glue: BridgethingGlue?) {
        TODO("Android implementation pending")
    }

    fun current(): BridgethingGlue? = activeGlue

    suspend fun setCapabilityFlags(flags: CompanionCapabilityFlags) {
        TODO("Android implementation pending")
    }

    /**
     * Subscribe to NowPlaying mirror updates from whichever glue is
     * active. Mirror of the Swift companion's `setNowPlayingObserver`.
     */
    suspend fun setNowPlayingObserver(observer: ((GlueNowPlaying?) -> Unit)?) {
        TODO("Android implementation pending")
    }
}
