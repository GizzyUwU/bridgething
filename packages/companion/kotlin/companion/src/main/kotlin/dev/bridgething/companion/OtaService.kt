package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.schema.OtaPhase
import java.io.File
import kotlinx.coroutines.flow.Flow

/**
 * Snapshot of the current OTA flow visible to the host app's UI.
 * Mirror of Swift `OtaPhaseSnapshot`.
 */
public sealed class OtaPhaseSnapshot {
    public object Idle : OtaPhaseSnapshot()
    public data class Streaming(val percent: Int) : OtaPhaseSnapshot()
    public data class Applying(val phase: OtaPhase, val percent: Int) : OtaPhaseSnapshot()
    public object Completed : OtaPhaseSnapshot()
    public data class Failed(val reason: String) : OtaPhaseSnapshot()
}

/**
 * Minimal OTA service wiring for the Android companion. Mirrors the
 * Swift `OtaService` actor: subscribe to inbound `OtaAssetRange`
 * requests and serve from a configured local `.zck` file, plus a
 * `pushUpdate(...)` driver that opens an OTA via `OtaBegin` and
 * streams the `.swu` as `OtaChunk` events.
 *
 * Implementation lands in a follow-up Android slice; the public shape
 * is stable enough now that the iOS companion + RN session API can be
 * designed against it.
 */
public class OtaService {
    public suspend fun start(gateway: BridgethingGateway) {
        TODO("Android implementation pending")
    }

    public suspend fun stop() {
        TODO("Android implementation pending")
    }

    public fun setLocalZck(file: File?) {
        TODO("Android implementation pending")
    }

    public fun currentLocalZck(): File? {
        TODO("Android implementation pending")
    }

    public suspend fun pushUpdate(
        gateway: BridgethingGateway,
        deviceId: String,
        swuPath: File,
        zckPath: File,
        updateUrlBase: String? = null,
    ): Flow<OtaPhaseSnapshot> {
        TODO("Android implementation pending")
    }
}
