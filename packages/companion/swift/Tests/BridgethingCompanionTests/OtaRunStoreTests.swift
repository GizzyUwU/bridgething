import BridgethingSchema
import Foundation
import XCTest
@testable import BridgethingCompanion

private let device = "AA:BB:CC:DD:EE:FF"

private func at(_ seconds: TimeInterval) -> Date {
  Date(timeIntervalSince1970: 1_700_000_000 + seconds)
}

private func planStep(_ id: Int, _ kind: OtaStepKind) -> OtaPlanStep {
  OtaPlanStep(id: id, kind: kind, label: "step", bytes: 100)
}

private func plannedImage(daemon: String = "0.9.0", image: String = "2026.06.0") -> OtaPollEvent {
  .planned(
    deviceId: device,
    kind: .image,
    release: "\(daemon)+image.\(image)",
    daemonVersion: daemon,
    imageVersion: image,
    steps: [planStep(0, .download), planStep(1, .apply), planStep(2, .reboot)]
  )
}

private func plannedWebapp() -> OtaPollEvent {
  .planned(
    deviceId: device,
    kind: .installedWebapp,
    release: "",
    daemonVersion: "",
    imageVersion: "",
    steps: [planStep(0, .download)]
  )
}

private func onlyRun(_ changes: [OtaStoreChange]) -> OtaRun? {
  for change in changes {
    if case let .run(run) = change { return run }
  }
  return nil
}

final class OtaRunStoreTests: XCTestCase {
  // MARK: - planned

  func testPlannedOpensARunCarryingItsPlan() {
    let store = OtaRunStore()
    let run = onlyRun(store.ingest(plannedImage(), now: at(0)))

    XCTAssertEqual(run?.deviceId, device)
    XCTAssertEqual(run?.kind, .image)
    XCTAssertEqual(run?.phase, .idle)
    XCTAssertEqual(run?.steps.count, 3)
    XCTAssertEqual(run?.stepId, 0)
    XCTAssertEqual(run?.daemonVersion, "0.9.0")
    XCTAssertEqual(run?.imageVersion, "2026.06.0")
    XCTAssertNil(run?.outcome, "a run that has only been planned has not ended")
    XCTAssertEqual(store.runs().count, 1)
  }

  func testPlannedWithNoVersionsLeavesThemUnsetRatherThanEmpty() {
    let store = OtaRunStore()
    let run = onlyRun(store.ingest(plannedWebapp(), now: at(0)))

    XCTAssertNil(run?.daemonVersion)
    XCTAssertNil(run?.imageVersion)
    XCTAssertNil(run?.releaseVersion)
  }

  func testASecondPlanReplacesTheFirstForTheSameDevice() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    let first = store.runs().first?.runId
    _ = store.ingest(plannedWebapp(), now: at(1))

    XCTAssertEqual(store.runs().count, 1, "the store keys one run per device")
    XCTAssertNotEqual(store.runs().first?.runId, first)
    XCTAssertEqual(store.runs().first?.kind, .installedWebapp)
  }

  // MARK: - progress

  func testProgressWithoutAPlanIsIgnored() {
    let store = OtaRunStore()
    let changes = store.ingest(
      .progress(deviceId: device, kind: .image, stepId: 0, snapshot: .staged),
      now: at(0)
    )

    XCTAssertTrue(changes.isEmpty)
    XCTAssertTrue(store.runs().isEmpty, "progress cannot conjure a run nobody planned")
  }

  func testDownloadProgressCarriesBytesAndRate() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    let run = onlyRun(store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 0,
        snapshot: .downloading(asset: "update.swu", received: 40, total: 100, ratePerSec: 20)
      ),
      now: at(1)
    ))

    XCTAssertEqual(run?.phase, .downloading)
    XCTAssertEqual(run?.stageReceived, 40)
    XCTAssertEqual(run?.stageTotal, 100)
    XCTAssertEqual(run?.ratePerSec, 20)
    XCTAssertEqual(run?.stepId, 0)
  }

  func testPhaseStartedAtMovesOnlyWhenThePhaseDoes() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    let first = onlyRun(store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 0,
        snapshot: .downloading(asset: "a", received: 1, total: 100, ratePerSec: nil)
      ),
      now: at(10)
    ))
    let same = onlyRun(store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 0,
        snapshot: .downloading(asset: "a", received: 2, total: 100, ratePerSec: nil)
      ),
      now: at(20)
    ))
    let next = onlyRun(store.ingest(
      .progress(deviceId: device, kind: .image, stepId: 1, snapshot: .staged),
      now: at(30)
    ))

    XCTAssertEqual(first?.phaseStartedAt, at(10))
    XCTAssertEqual(same?.phaseStartedAt, at(10), "more of the same phase is not a new phase")
    XCTAssertEqual(next?.phaseStartedAt, at(30))
  }

  func testApplyingReportsDeltaBytesOnlyWhileTheDeltaPullIsMeasurable() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))

    let pulling = onlyRun(store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 1,
        snapshot: .applying(phase: .writing, writePercent: 10, dwlPercent: 50, dwlBytes: 4096)
      ),
      now: at(1)
    ))
    XCTAssertEqual(pulling?.phase, .writing)
    XCTAssertEqual(pulling?.dwlPercent, 50)
    XCTAssertEqual(pulling?.stageReceived, 4096)

    let writing = onlyRun(store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 1,
        snapshot: .applying(phase: .writing, writePercent: 60, dwlPercent: 100, dwlBytes: 8192)
      ),
      now: at(2)
    ))
    XCTAssertNil(
      writing?.stageReceived,
      "past the delta pull there are no reported bytes, and a frozen number reads as a stall"
    )
  }

  // MARK: - terminal events

  func testUpdatedEndsTheRunAndClearsTheAvailableUpdate() {
    let store = OtaRunStore()
    _ = store.ingest(
      .updateAvailable(deviceId: device, release: "r", daemonVersion: "0.9.0", imageVersion: "2026.06.0"),
      now: at(0)
    )
    _ = store.ingest(plannedImage(), now: at(1))
    _ = store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 0,
        snapshot: .downloading(asset: "a", received: 5, total: 100, ratePerSec: 3)
      ),
      now: at(2)
    )
    XCTAssertEqual(store.available().count, 1, "an update was on offer before it was installed")
    let changes = store.ingest(.updated(deviceId: device, kind: .image, version: "2026.06.0"), now: at(3))
    let run = onlyRun(changes)

    XCTAssertEqual(run?.phase, .completed)
    XCTAssertEqual(run?.outcome, .succeeded)
    XCTAssertNil(run?.stageReceived, "leftover progress would render behind a finished bar")
    XCTAssertNil(run?.ratePerSec)
    XCTAssertTrue(store.available().isEmpty, "the update is no longer available; it is installed")

    var cleared: OtaAvailable?
    for change in changes {
      if case let .available(entry) = change { cleared = entry }
    }
    XCTAssertNil(
      cleared?.releaseVersion,
      "listeners are told the offer is gone, not left holding the old one"
    )
    XCTAssertNotNil(cleared, "the cleared offer is announced, not silently dropped")
  }

  func testFailedRecordsTheReason() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    let run = onlyRun(store.ingest(
      .failed(deviceId: device, kind: .image, reason: "write failed"),
      now: at(1)
    ))

    XCTAssertEqual(run?.phase, .failed)
    XCTAssertEqual(run?.outcome, .failed)
    XCTAssertEqual(run?.error, "write failed")
  }

  func testCancellationIsItsOwnOutcome() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    let run = onlyRun(store.ingest(
      .failed(deviceId: device, kind: .image, reason: cancelledReason),
      now: at(1)
    ))

    XCTAssertEqual(run?.outcome, .cancelled, "a user stopping an update is not a failure to report")
  }

  func testAFailureWithNoPlanStillOpensARunToReportIt() {
    let store = OtaRunStore()
    let run = onlyRun(store.ingest(
      .failed(deviceId: device, kind: .daemon, reason: "gateway not attached"),
      now: at(0)
    ))

    XCTAssertEqual(run?.outcome, .failed)
    XCTAssertEqual(run?.kind, .daemon)
    XCTAssertTrue(run?.steps.isEmpty ?? false)
  }

  // MARK: - dismiss

  func testDismissClearsAFinishedRun() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(.updated(deviceId: device, kind: .image, version: "2026.06.0"), now: at(1))

    XCTAssertNotNil(store.dismiss(deviceId: device))
    XCTAssertTrue(store.runs().isEmpty)
  }

  func testDismissRefusesARunStillInFlight() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))

    XCTAssertNil(
      store.dismiss(deviceId: device),
      "dismissing a card must not make the update it describes invisible"
    )
    XCTAssertEqual(store.runs().count, 1)
  }

  func testAnAbandonedRunIsReportedAndThenDismissable() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    XCTAssertEqual(store.openRunKind(deviceId: device), .image, "a planned run has not ended")
    XCTAssertNil(store.dismiss(deviceId: device), "and cannot be dismissed while it is open")

    _ = store.ingest(.failed(deviceId: device, kind: .image, reason: "abandoned"), now: at(1))

    XCTAssertNil(store.openRunKind(deviceId: device), "the run has ended")
    XCTAssertNotNil(store.dismiss(deviceId: device), "so the card can be cleared")
    XCTAssertTrue(store.runs().isEmpty)
  }

  func testOpenRunKindIgnoresAFinishedRun() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(.updated(deviceId: device, kind: .image, version: "2026.06.0"), now: at(1))

    XCTAssertNil(
      store.openRunKind(deviceId: device),
      "a run that reported a result needs no backstop terminal"
    )
  }

  func testProgressForAStepOutsideThePlanKeepsTheLastUnderstoodPosition() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 1,
        snapshot: .applying(phase: .writing, writePercent: 50, dwlPercent: 50, dwlBytes: 0)
      ),
      now: at(1)
    )

    let run = onlyRun(store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 99,
        snapshot: .applying(phase: .writing, writePercent: 60, dwlPercent: 60, dwlBytes: 0)
      ),
      now: at(2)
    ))

    XCTAssertEqual(
      run?.stepId, 1,
      "a step id that does not index this plan must not rewind the run to its start"
    )
  }

  // MARK: - interrupt

  func testALinkThatDiesMidDownloadEndsTheRun() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 0,
        snapshot: .downloading(asset: "update.swu", received: 40, total: 100, ratePerSec: 20)
      ),
      now: at(1)
    )

    let interrupted = store.interrupt(deviceId: device)

    XCTAssertEqual(interrupted?.outcome, .failed)
    XCTAssertNotNil(store.dismiss(deviceId: device), "so the card can be cleared")
  }

  func testALinkThatDiesWhileTheDeviceRebootsLeavesTheRunAlone() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(
      .progress(
        deviceId: device, kind: .image, stepId: 2,
        snapshot: .applying(phase: .reboot, writePercent: 100, dwlPercent: 100, dwlBytes: 0)
      ),
      now: at(1)
    )

    XCTAssertNil(
      store.interrupt(deviceId: device),
      "the run is what asked the device to go away, so its disconnect is not a failure"
    )
    XCTAssertNil(store.runs().first?.outcome)
  }

  func testInterruptLeavesAFinishedRunAlone() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(.updated(deviceId: device, kind: .image, version: "2026.06.0"), now: at(1))

    XCTAssertNil(store.interrupt(deviceId: device))
    XCTAssertEqual(store.runs().first?.outcome, .succeeded)
  }

  // MARK: - noteMeta

  func testMetaOnTheTargetVersionClearsTheRun() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(.updated(deviceId: device, kind: .image, version: "2026.06.0"), now: at(1))

    let cleared = store.noteMeta(deviceId: device, daemonVersion: "0.9.0", imageVersion: "2026.06.0")

    XCTAssertEqual(cleared?.phase, .idle)
    XCTAssertTrue(store.runs().isEmpty)
  }

  func testMetaOnTheWrongVersionLeavesTheRunAlone() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(.updated(deviceId: device, kind: .image, version: "2026.06.0"), now: at(1))

    XCTAssertNil(store.noteMeta(deviceId: device, daemonVersion: "0.9.0", imageVersion: "2026.05.0"))
    XCTAssertEqual(store.runs().count, 1, "the device came back on the old image; the run did not land")
  }

  func testMetaOnTheTargetVersionRescuesARunThatTimedOutRebooting() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(
      .failed(deviceId: device, kind: .image, reason: "ota stalled: no progress within 60s"),
      now: at(90)
    )
    XCTAssertEqual(store.runs().first?.outcome, .failed)

    let cleared = store.noteMeta(deviceId: device, daemonVersion: "0.9.0", imageVersion: "2026.06.0")

    XCTAssertEqual(cleared?.outcome, .succeeded, "the version the device came back on outranks the guess")
    XCTAssertNil(cleared?.error)
    XCTAssertTrue(store.runs().isEmpty)
  }

  func testMetaDoesNotRescueARunThatFailedBeforeReachingTheDevice() {
    let store = OtaRunStore()
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(
      .failed(deviceId: device, kind: .image, reason: "bundle download failed"),
      now: at(1)
    )

    XCTAssertNil(
      store.noteMeta(deviceId: device, daemonVersion: "0.8.0", imageVersion: "2026.05.0"),
      "the device is still on the old versions, so nothing confirms this run"
    )
    XCTAssertEqual(store.runs().count, 1)
  }

  func testMetaLeavesAWebappRunAloneUntilItSaysItSucceeded() {
    let store = OtaRunStore()
    _ = store.ingest(plannedWebapp(), now: at(0))

    XCTAssertNil(
      store.noteMeta(deviceId: device, daemonVersion: "0.9.0", imageVersion: "2026.06.0"),
      "a webapp install targets no version, so device meta confirms nothing about it"
    )
    XCTAssertEqual(store.runs().count, 1)
  }

  // MARK: - annotateWebapp

  func testAnnotateNamesTheAppBeingInstalled() {
    let store = OtaRunStore()
    _ = store.ingest(plannedWebapp(), now: at(0))
    let run = store.annotateWebapp(deviceId: device, webappId: "abc", webappName: "Weather")

    XCTAssertEqual(run?.webappName, "Weather")
    XCTAssertEqual(store.runs().first?.webappId, "abc")
  }

  func testAnnotateWithoutARunIsANoOp() {
    let store = OtaRunStore()
    XCTAssertNil(store.annotateWebapp(deviceId: device, webappId: "abc", webappName: "Weather"))
  }

  func testAWebappRunCarriesItsNameAndItsInstalledVersionSeparately() {
    let store = OtaRunStore()
    _ = store.ingest(plannedWebapp(), now: at(0))
    _ = store.annotateWebapp(deviceId: device, webappId: "abc", webappName: "Weather")
    let run = onlyRun(store.ingest(.updated(deviceId: device, kind: .installedWebapp, version: "1.4.0"), now: at(1)))

    XCTAssertEqual(run?.webappName, "Weather", "the app's name identifies it")
    XCTAssertEqual(
      run?.releaseVersion,
      "1.4.0",
      "and the version field holds a version, not the name again"
    )
  }

  // MARK: - poll status

  func testPollFailureKeepsTheLastGoodTimestamp() {
    let store = OtaRunStore()
    _ = store.ingest(.manifestPolled(updatedAt: "2026-06-01T00:00:00Z"), now: at(0))
    _ = store.ingest(.manifestPollFailed(reason: "offline"), now: at(1))

    XCTAssertEqual(store.pollStatus().lastPolledAt, "2026-06-01T00:00:00Z")
    XCTAssertEqual(store.pollStatus().error, "offline")
  }

  func testASuccessfulPollClearsThePreviousError() {
    let store = OtaRunStore()
    _ = store.ingest(.manifestPollFailed(reason: "offline"), now: at(0))
    _ = store.ingest(.manifestPolled(updatedAt: "2026-06-02T00:00:00Z"), now: at(1))

    XCTAssertNil(store.pollStatus().error)
    XCTAssertEqual(store.pollStatus().lastPolledAt, "2026-06-02T00:00:00Z")
  }

  // MARK: - isolation

  func testRunsOnDifferentDevicesDoNotInterfere() {
    let store = OtaRunStore()
    let other = "11:22:33:44:55:66"
    _ = store.ingest(plannedImage(), now: at(0))
    _ = store.ingest(
      .planned(
        deviceId: other, kind: .daemon, release: "0.9.1",
        daemonVersion: "0.9.1", imageVersion: "", steps: [planStep(0, .download)]
      ),
      now: at(1)
    )
    _ = store.ingest(.failed(deviceId: other, kind: .daemon, reason: "nope"), now: at(2))

    XCTAssertEqual(store.runs().count, 2)
    XCTAssertNil(store.runs().first { $0.deviceId == device }?.outcome)
    XCTAssertEqual(store.runs().first { $0.deviceId == other }?.outcome, .failed)
  }
}
