package com.bridgething.companion

import com.bridgething.schema.OtaKind
import com.bridgething.schema.OtaPhase
import java.util.UUID

public enum class OtaRunOutcome { SUCCEEDED, FAILED, CANCELLED }

public enum class OtaRunPhase {
    IDLE, DOWNLOADING, STREAMING, VERIFYING, WRITING, CONFIRMING, REBOOT, COMPLETED, FAILED
}

public data class OtaRun(
    val runId: String,
    val deviceId: String,
    val kind: OtaKind,
    val phase: OtaRunPhase,
    val steps: List<OtaPlanStep>,
    val stepId: Int,
    val startedAt: Long,
    val phaseStartedAt: Long,
    val stageReceived: Long? = null,
    val stageTotal: Long? = null,
    val ratePerSec: Double? = null,
    val dwlPercent: Int? = null,
    val outcome: OtaRunOutcome? = null,
    val error: String? = null,
    val releaseVersion: String? = null,
    val daemonVersion: String? = null,
    val imageVersion: String? = null,
    val webappId: String? = null,
    val webappName: String? = null,
)

public data class OtaAvailable(
    val deviceId: String,
    val releaseVersion: String? = null,
    val daemonVersion: String? = null,
    val imageVersion: String? = null,
)

public data class OtaPollStatus(
    val lastPolledAt: String? = null,
    val error: String? = null,
)

public sealed class OtaStoreChange {
    public data class Run(val run: OtaRun) : OtaStoreChange()
    public data class Available(val available: OtaAvailable) : OtaStoreChange()
    public data class Poll(val status: OtaPollStatus) : OtaStoreChange()
}

internal const val CANCELLED_REASON: String = "cancelled"

internal class OtaRunStore {
    private val lock = Any()
    private val runsByDevice = mutableMapOf<String, OtaRun>()
    private val availableByDevice = mutableMapOf<String, OtaAvailable>()
    private var poll = OtaPollStatus()

    fun runs(): List<OtaRun> = synchronized(lock) { runsByDevice.values.toList() }

    fun available(): List<OtaAvailable> = synchronized(lock) { availableByDevice.values.toList() }

    fun pollStatus(): OtaPollStatus = synchronized(lock) { poll }

    fun dismiss(deviceId: String): OtaRun? = synchronized(lock) {
        val run = runsByDevice[deviceId] ?: return null
        if (run.outcome == null) return null
        runsByDevice.remove(deviceId)
        run.copy(phase = OtaRunPhase.IDLE)
    }

    fun noteMeta(deviceId: String, daemonVersion: String, imageVersion: String): OtaRun? = synchronized(lock) {
        val run = runsByDevice[deviceId] ?: return null
        val daemonOk = run.daemonVersion == null || run.daemonVersion == daemonVersion
        val imageOk = run.imageVersion == null || run.imageVersion == imageVersion
        if (!daemonOk || !imageOk) return null
        val targeted = run.daemonVersion != null || run.imageVersion != null
        if (!targeted && run.outcome != OtaRunOutcome.SUCCEEDED) return null
        runsByDevice.remove(deviceId)
        run.copy(phase = OtaRunPhase.IDLE, outcome = OtaRunOutcome.SUCCEEDED, error = null)
    }

    fun openRunKind(deviceId: String): OtaKind? = synchronized(lock) {
        runsByDevice[deviceId]?.takeIf { it.outcome == null }?.kind
    }

    fun annotateWebapp(deviceId: String, webappId: String?, webappName: String?): OtaRun? = synchronized(lock) {
        val run = runsByDevice[deviceId] ?: return null
        val next = run.copy(webappId = webappId, webappName = webappName)
        runsByDevice[deviceId] = next
        next
    }

    fun ingest(event: OtaPollEvent, now: Long): List<OtaStoreChange> = synchronized(lock) {
        when (event) {
            is OtaPollEvent.ManifestPolled -> {
                poll = OtaPollStatus(lastPolledAt = event.updatedAt, error = null)
                listOf(OtaStoreChange.Poll(poll))
            }

            is OtaPollEvent.ManifestPollFailed -> {
                poll = poll.copy(error = event.reason)
                listOf(OtaStoreChange.Poll(poll))
            }

            is OtaPollEvent.UpdateAvailable -> {
                val entry = OtaAvailable(
                    deviceId = event.deviceId,
                    releaseVersion = event.release,
                    daemonVersion = event.daemonVersion,
                    imageVersion = event.imageVersion,
                )
                availableByDevice[event.deviceId] = entry
                listOf(OtaStoreChange.Available(entry))
            }

            is OtaPollEvent.Planned -> {
                val run = OtaRun(
                    runId = UUID.randomUUID().toString(),
                    deviceId = event.deviceId,
                    kind = event.kind,
                    phase = OtaRunPhase.IDLE,
                    steps = event.steps,
                    stepId = event.steps.firstOrNull()?.id ?: 0,
                    startedAt = now,
                    phaseStartedAt = now,
                    releaseVersion = event.release.ifEmpty { null },
                    daemonVersion = event.daemonVersion.ifEmpty { null },
                    imageVersion = event.imageVersion.ifEmpty { null },
                )
                runsByDevice[event.deviceId] = run
                listOf(OtaStoreChange.Run(run))
            }

            is OtaPollEvent.Progress -> {
                val prev = runsByDevice[event.deviceId] ?: return emptyList()
                val applied = apply(event.snapshot, prev.copy(kind = event.kind, stepId = event.stepId))
                val next = if (applied.phase != prev.phase) applied.copy(phaseStartedAt = now) else applied
                runsByDevice[event.deviceId] = next
                listOf(OtaStoreChange.Run(next))
            }

            is OtaPollEvent.Updated -> {
                val prev = runsByDevice[event.deviceId] ?: return emptyList()
                val next = prev.copy(
                    phase = OtaRunPhase.COMPLETED,
                    outcome = OtaRunOutcome.SUCCEEDED,
                    error = null,
                    stageReceived = null,
                    stageTotal = null,
                    ratePerSec = null,
                    dwlPercent = null,
                    releaseVersion = prev.releaseVersion ?: event.version.ifEmpty { null },
                )
                runsByDevice[event.deviceId] = next
                availableByDevice.remove(event.deviceId)
                listOf(OtaStoreChange.Run(next), OtaStoreChange.Available(OtaAvailable(event.deviceId)))
            }

            is OtaPollEvent.Failed -> {
                val prev = runsByDevice[event.deviceId] ?: OtaRun(
                    runId = UUID.randomUUID().toString(),
                    deviceId = event.deviceId,
                    kind = event.kind,
                    phase = OtaRunPhase.FAILED,
                    steps = emptyList(),
                    stepId = 0,
                    startedAt = now,
                    phaseStartedAt = now,
                )
                val next = prev.copy(
                    phase = OtaRunPhase.FAILED,
                    outcome = if (event.reason == CANCELLED_REASON) OtaRunOutcome.CANCELLED else OtaRunOutcome.FAILED,
                    error = event.reason,
                    stageReceived = null,
                    stageTotal = null,
                    ratePerSec = null,
                )
                runsByDevice[event.deviceId] = next
                listOf(OtaStoreChange.Run(next))
            }
        }
    }

    private fun apply(snapshot: OtaPhaseSnapshot, run: OtaRun): OtaRun = when (snapshot) {
        is OtaPhaseSnapshot.Idle -> run.copy(phase = OtaRunPhase.IDLE)

        is OtaPhaseSnapshot.Downloading -> run.copy(
            phase = OtaRunPhase.DOWNLOADING,
            stageReceived = snapshot.received,
            stageTotal = snapshot.total,
            ratePerSec = snapshot.ratePerSec,
        )

        is OtaPhaseSnapshot.Streaming -> run.copy(
            phase = OtaRunPhase.STREAMING,
            stageReceived = snapshot.sent,
            stageTotal = snapshot.total,
            ratePerSec = snapshot.ratePerSec,
        )

        is OtaPhaseSnapshot.Applying -> run.copy(
            phase = when (snapshot.phase) {
                OtaPhase.Streaming -> OtaRunPhase.STREAMING
                OtaPhase.Verifying -> OtaRunPhase.VERIFYING
                OtaPhase.Writing -> OtaRunPhase.WRITING
                OtaPhase.Confirming -> OtaRunPhase.CONFIRMING
                OtaPhase.Reboot -> OtaRunPhase.REBOOT
            },
            dwlPercent = snapshot.dwlPercent,
            stageReceived = if (snapshot.dwlPercent < 100 && snapshot.dwlBytes > 0) snapshot.dwlBytes else null,
            stageTotal = null,
        )

        is OtaPhaseSnapshot.Staged -> run.copy(
            phase = OtaRunPhase.WRITING,
            stageReceived = null,
            stageTotal = null,
        )

        is OtaPhaseSnapshot.Completed -> run.copy(phase = OtaRunPhase.COMPLETED)

        is OtaPhaseSnapshot.Failed -> run.copy(phase = OtaRunPhase.FAILED, error = snapshot.reason)
    }
}
