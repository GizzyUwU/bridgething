package com.bridgething.companion

import com.bridgething.schema.OtaKind
import com.bridgething.schema.OtaPhase
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

private const val DEVICE = "AA:BB:CC:DD:EE:FF"
private const val EPOCH = 1_700_000_000_000L

private fun at(seconds: Long): Long = EPOCH + seconds * 1_000L

private fun planStep(id: Int, kind: OtaStepKind) = OtaPlanStep(id, kind, "step", 100L)

private fun plannedImage(daemon: String = "0.9.0", image: String = "2026.06.0") = OtaPollEvent.Planned(
    deviceId = DEVICE,
    kind = OtaKind.Image,
    release = "$daemon+image.$image",
    daemonVersion = daemon,
    imageVersion = image,
    steps = listOf(planStep(0, OtaStepKind.DOWNLOAD), planStep(1, OtaStepKind.APPLY), planStep(2, OtaStepKind.REBOOT)),
)

private fun plannedWebapp() = OtaPollEvent.Planned(
    deviceId = DEVICE,
    kind = OtaKind.InstalledWebapp,
    release = "",
    daemonVersion = "",
    imageVersion = "",
    steps = listOf(planStep(0, OtaStepKind.DOWNLOAD)),
)

private fun onlyRun(changes: List<OtaStoreChange>): OtaRun? =
    changes.filterIsInstance<OtaStoreChange.Run>().firstOrNull()?.run

class OtaRunStoreTest {
    // planned

    @Test
    fun `planned opens a run carrying its plan`() {
        val store = OtaRunStore()
        val run = onlyRun(store.ingest(plannedImage(), at(0)))

        assertEquals(DEVICE, run?.deviceId)
        assertEquals(OtaKind.Image, run?.kind)
        assertEquals(OtaRunPhase.IDLE, run?.phase)
        assertEquals(3, run?.steps?.size)
        assertEquals(0, run?.stepId)
        assertEquals("0.9.0", run?.daemonVersion)
        assertEquals("2026.06.0", run?.imageVersion)
        assertNull(run?.outcome, "a run that has only been planned has not ended")
        assertEquals(1, store.runs().size)
    }

    @Test
    fun `planned with no versions leaves them unset rather than empty`() {
        val store = OtaRunStore()
        val run = onlyRun(store.ingest(plannedWebapp(), at(0)))

        assertNull(run?.daemonVersion)
        assertNull(run?.imageVersion)
        assertNull(run?.releaseVersion)
    }

    @Test
    fun `a second plan replaces the first for the same device`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        val first = store.runs().first().runId
        store.ingest(plannedWebapp(), at(1))

        assertEquals(1, store.runs().size, "the store keys one run per device")
        assertTrue(store.runs().first().runId != first)
        assertEquals(OtaKind.InstalledWebapp, store.runs().first().kind)
    }

    // progress

    @Test
    fun `progress without a plan is ignored`() {
        val store = OtaRunStore()
        val changes = store.ingest(
            OtaPollEvent.Progress(DEVICE, OtaKind.Image, 0, OtaPhaseSnapshot.Staged),
            at(0),
        )

        assertTrue(changes.isEmpty())
        assertTrue(store.runs().isEmpty(), "progress cannot conjure a run nobody planned")
    }

    @Test
    fun `download progress carries bytes and rate`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        val run = onlyRun(
            store.ingest(
                OtaPollEvent.Progress(
                    DEVICE, OtaKind.Image, 0,
                    OtaPhaseSnapshot.Downloading("update.swu", 40L, 100L, 20.0),
                ),
                at(1),
            )
        )

        assertEquals(OtaRunPhase.DOWNLOADING, run?.phase)
        assertEquals(40L, run?.stageReceived)
        assertEquals(100L, run?.stageTotal)
        assertEquals(20.0, run?.ratePerSec)
        assertEquals(0, run?.stepId)
    }

    @Test
    fun `phase started at moves only when the phase does`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        val first = onlyRun(
            store.ingest(
                OtaPollEvent.Progress(
                    DEVICE, OtaKind.Image, 0, OtaPhaseSnapshot.Downloading("a", 1L, 100L, null),
                ),
                at(10),
            )
        )
        val same = onlyRun(
            store.ingest(
                OtaPollEvent.Progress(
                    DEVICE, OtaKind.Image, 0, OtaPhaseSnapshot.Downloading("a", 2L, 100L, null),
                ),
                at(20),
            )
        )
        val next = onlyRun(
            store.ingest(OtaPollEvent.Progress(DEVICE, OtaKind.Image, 1, OtaPhaseSnapshot.Staged), at(30))
        )

        assertEquals(at(10), first?.phaseStartedAt)
        assertEquals(at(10), same?.phaseStartedAt, "more of the same phase is not a new phase")
        assertEquals(at(30), next?.phaseStartedAt)
    }

    @Test
    fun `applying reports delta bytes only while the delta pull is measurable`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))

        val pulling = onlyRun(
            store.ingest(
                OtaPollEvent.Progress(
                    DEVICE, OtaKind.Image, 1, OtaPhaseSnapshot.Applying(OtaPhase.Writing, 10, 50, 4096L),
                ),
                at(1),
            )
        )
        assertEquals(OtaRunPhase.WRITING, pulling?.phase)
        assertEquals(50, pulling?.dwlPercent)
        assertEquals(4096L, pulling?.stageReceived)

        val writing = onlyRun(
            store.ingest(
                OtaPollEvent.Progress(
                    DEVICE, OtaKind.Image, 1, OtaPhaseSnapshot.Applying(OtaPhase.Writing, 60, 100, 8192L),
                ),
                at(2),
            )
        )
        assertNull(
            writing?.stageReceived,
            "past the delta pull there are no reported bytes, and a frozen number reads as a stall",
        )
    }

    // terminal events

    @Test
    fun `updated ends the run and clears the available update`() {
        val store = OtaRunStore()
        store.ingest(OtaPollEvent.UpdateAvailable(DEVICE, "r", "0.9.0", "2026.06.0"), at(0))
        store.ingest(plannedImage(), at(1))
        store.ingest(
            OtaPollEvent.Progress(DEVICE, OtaKind.Image, 0, OtaPhaseSnapshot.Downloading("a", 5L, 100L, 3.0)),
            at(2),
        )
        assertEquals(1, store.available().size, "an update was on offer before it was installed")
        val changes = store.ingest(OtaPollEvent.Updated(DEVICE, OtaKind.Image, "2026.06.0"), at(3))
        val run = onlyRun(changes)

        assertEquals(OtaRunPhase.COMPLETED, run?.phase)
        assertEquals(OtaRunOutcome.SUCCEEDED, run?.outcome)
        assertNull(run?.stageReceived, "leftover progress would render behind a finished bar")
        assertNull(run?.ratePerSec)
        assertTrue(store.available().isEmpty(), "the update is no longer available; it is installed")
        val cleared = changes.filterIsInstance<OtaStoreChange.Available>().single().available
        assertNull(cleared.releaseVersion, "listeners are told the offer is gone, not left holding the old one")
    }

    @Test
    fun `failed records the reason`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        val run = onlyRun(store.ingest(OtaPollEvent.Failed(DEVICE, OtaKind.Image, "write failed"), at(1)))

        assertEquals(OtaRunPhase.FAILED, run?.phase)
        assertEquals(OtaRunOutcome.FAILED, run?.outcome)
        assertEquals("write failed", run?.error)
    }

    @Test
    fun `cancellation is its own outcome`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        val run = onlyRun(store.ingest(OtaPollEvent.Failed(DEVICE, OtaKind.Image, CANCELLED_REASON), at(1)))

        assertEquals(OtaRunOutcome.CANCELLED, run?.outcome, "a user stopping an update is not a failure to report")
    }

    @Test
    fun `a failure with no plan still opens a run to report it`() {
        val store = OtaRunStore()
        val run = onlyRun(store.ingest(OtaPollEvent.Failed(DEVICE, OtaKind.Daemon, "gateway not attached"), at(0)))

        assertEquals(OtaRunOutcome.FAILED, run?.outcome)
        assertEquals(OtaKind.Daemon, run?.kind)
        assertTrue(run?.steps?.isEmpty() ?: false)
    }

    // dismiss

    @Test
    fun `dismiss clears a finished run`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        store.ingest(OtaPollEvent.Updated(DEVICE, OtaKind.Image, "2026.06.0"), at(1))

        assertTrue(store.dismiss(DEVICE) != null)
        assertTrue(store.runs().isEmpty())
    }

    @Test
    fun `dismiss refuses a run still in flight`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))

        assertNull(store.dismiss(DEVICE), "dismissing a card must not make the update it describes invisible")
        assertEquals(1, store.runs().size)
    }

    @Test
    fun `an abandoned run is reported and then dismissable`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        assertEquals(OtaKind.Image, store.openRunKind(DEVICE), "a planned run has not ended")
        assertNull(store.dismiss(DEVICE), "and cannot be dismissed while it is open")

        store.ingest(OtaPollEvent.Failed(DEVICE, OtaKind.Image, "abandoned"), at(1))

        assertNull(store.openRunKind(DEVICE), "the run has ended")
        assertTrue(store.dismiss(DEVICE) != null, "so the card can be cleared")
        assertTrue(store.runs().isEmpty())
    }

    @Test
    fun `open run kind ignores a finished run`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        store.ingest(OtaPollEvent.Updated(DEVICE, OtaKind.Image, "2026.06.0"), at(1))

        assertNull(store.openRunKind(DEVICE), "a run that reported a result needs no backstop terminal")
    }

    // noteMeta

    @Test
    fun `meta on the target version clears the run`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        store.ingest(OtaPollEvent.Updated(DEVICE, OtaKind.Image, "2026.06.0"), at(1))

        val cleared = store.noteMeta(DEVICE, "0.9.0", "2026.06.0")

        assertEquals(OtaRunPhase.IDLE, cleared?.phase)
        assertTrue(store.runs().isEmpty())
    }

    @Test
    fun `meta on the wrong version leaves the run alone`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        store.ingest(OtaPollEvent.Updated(DEVICE, OtaKind.Image, "2026.06.0"), at(1))

        assertNull(store.noteMeta(DEVICE, "0.9.0", "2026.05.0"))
        assertEquals(1, store.runs().size, "the device came back on the old image; the run did not land")
    }

    @Test
    fun `meta on the target version rescues a run that timed out rebooting`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        store.ingest(OtaPollEvent.Failed(DEVICE, OtaKind.Image, "ota stalled: no progress within 60s"), at(90))
        assertEquals(OtaRunOutcome.FAILED, store.runs().first().outcome)

        val cleared = store.noteMeta(DEVICE, "0.9.0", "2026.06.0")

        assertEquals(OtaRunOutcome.SUCCEEDED, cleared?.outcome, "the version the device came back on outranks the guess")
        assertNull(cleared?.error)
        assertTrue(store.runs().isEmpty())
    }

    @Test
    fun `meta does not rescue a run that failed before reaching the device`() {
        val store = OtaRunStore()
        store.ingest(plannedImage(), at(0))
        store.ingest(OtaPollEvent.Failed(DEVICE, OtaKind.Image, "bundle download failed"), at(1))

        assertNull(
            store.noteMeta(DEVICE, "0.8.0", "2026.05.0"),
            "the device is still on the old versions, so nothing confirms this run",
        )
        assertEquals(1, store.runs().size)
    }

    @Test
    fun `meta leaves a webapp run alone until it says it succeeded`() {
        val store = OtaRunStore()
        store.ingest(plannedWebapp(), at(0))

        assertNull(
            store.noteMeta(DEVICE, "0.9.0", "2026.06.0"),
            "a webapp install targets no version, so device meta confirms nothing about it",
        )
        assertEquals(1, store.runs().size)
    }

    // annotateWebapp

    @Test
    fun `annotate names the app being installed`() {
        val store = OtaRunStore()
        store.ingest(plannedWebapp(), at(0))
        val run = store.annotateWebapp(DEVICE, "abc", "Weather")

        assertEquals("Weather", run?.webappName)
        assertEquals("abc", store.runs().first().webappId)
    }

    @Test
    fun `annotate without a run is a no op`() {
        val store = OtaRunStore()
        assertNull(store.annotateWebapp(DEVICE, "abc", "Weather"))
    }

    @Test
    fun `a webapp run carries its name and its installed version separately`() {
        val store = OtaRunStore()
        store.ingest(plannedWebapp(), at(0))
        store.annotateWebapp(DEVICE, "abc", "Weather")
        val run = onlyRun(store.ingest(OtaPollEvent.Updated(DEVICE, OtaKind.InstalledWebapp, "1.4.0"), at(1)))

        assertEquals("Weather", run?.webappName, "the app's name identifies it")
        assertEquals("1.4.0", run?.releaseVersion, "and the version field holds a version, not the name again")
    }

    // poll status

    @Test
    fun `poll failure keeps the last good timestamp`() {
        val store = OtaRunStore()
        store.ingest(OtaPollEvent.ManifestPolled("2026-06-01T00:00:00Z"), at(0))
        store.ingest(OtaPollEvent.ManifestPollFailed("offline"), at(1))

        assertEquals("2026-06-01T00:00:00Z", store.pollStatus().lastPolledAt)
        assertEquals("offline", store.pollStatus().error)
    }

    @Test
    fun `a successful poll clears the previous error`() {
        val store = OtaRunStore()
        store.ingest(OtaPollEvent.ManifestPollFailed("offline"), at(0))
        store.ingest(OtaPollEvent.ManifestPolled("2026-06-02T00:00:00Z"), at(1))

        assertNull(store.pollStatus().error)
        assertEquals("2026-06-02T00:00:00Z", store.pollStatus().lastPolledAt)
    }

    // isolation

    @Test
    fun `runs on different devices do not interfere`() {
        val store = OtaRunStore()
        val other = "11:22:33:44:55:66"
        store.ingest(plannedImage(), at(0))
        store.ingest(
            OtaPollEvent.Planned(other, OtaKind.Daemon, "0.9.1", "0.9.1", "", listOf(planStep(0, OtaStepKind.DOWNLOAD))),
            at(1),
        )
        store.ingest(OtaPollEvent.Failed(other, OtaKind.Daemon, "nope"), at(2))

        assertEquals(2, store.runs().size)
        assertNull(store.runs().first { it.deviceId == DEVICE }.outcome)
        assertEquals(OtaRunOutcome.FAILED, store.runs().first { it.deviceId == other }.outcome)
    }
}
