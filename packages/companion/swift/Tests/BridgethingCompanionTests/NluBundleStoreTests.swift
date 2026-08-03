import Foundation
import Testing
import ZIPFoundation

@testable import BridgethingCompanion

private final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var value = 0

    func bump() {
        lock.lock()
        defer { lock.unlock() }
        value += 1
    }

    var count: Int {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

private struct FakeTransport: NluBundleTransport {
    let manifests: @Sendable () -> NluBundleManifest
    let archives: @Sendable (String) -> URL
    let downloads: Counter

    func manifest(from _: URL) async throws -> NluBundleManifest { manifests() }

    func download(
        _ artifact: NluBundleArtifact,
        into directory: URL,
        onProgress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> URL {
        downloads.bump()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let dest = directory.appendingPathComponent("bundle-\(artifact.sha256).zip")
        try? FileManager.default.removeItem(at: dest)
        try FileManager.default.copyItem(at: archives(artifact.sha256), to: dest)
        onProgress(artifact.size, artifact.size)
        return dest
    }
}

private func scratch() -> URL {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent("nlu-store-test-\(UUID().uuidString)", isDirectory: true)
    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
}

private func makeBundleZip(in dir: URL, name: String, entries: [String] = ["manifest.json", "tokenizer.json"]) throws -> URL {
    let source = dir.appendingPathComponent("src-\(name)", isDirectory: true)
    try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
    for entry in entries {
        try Data("{}".utf8).write(to: source.appendingPathComponent(entry))
    }
    let mlpackage = source.appendingPathComponent("model.mlpackage", isDirectory: true)
    try FileManager.default.createDirectory(at: mlpackage, withIntermediateDirectories: true)
    try Data("{}".utf8).write(to: mlpackage.appendingPathComponent("Manifest.json"))

    let zip = dir.appendingPathComponent("\(name).zip")
    try? FileManager.default.removeItem(at: zip)
    try FileManager.default.zipItem(at: source, to: zip, shouldKeepParent: false)
    return zip
}

private func manifest(version: String, sha: String) -> NluBundleManifest {
    let json = """
    {
      "version": "\(version)",
      "updated_at": "2026-08-02T00:00:00Z",
      "ios": {
        "url": "https://ota.bridgething.com/nlu/stable/bundle/\(version)/bundle-ios.zip",
        "size": 1024,
        "sha256": "\(sha)"
      },
      "android": {
        "url": "https://ota.bridgething.com/nlu/stable/bundle/\(version)/bundle-android.zip",
        "size": 512,
        "sha256": "android-\(sha)"
      }
    }
    """
    return try! JSONDecoder().decode(NluBundleManifest.self, from: Data(json.utf8))
}

@Suite("nlu bundle store")
struct NluBundleStoreTests {
    @Test("a fresh bundle validates and rotates into place")
    func freshInstall() async throws {
        let dir = scratch()
        let zip = try makeBundleZip(in: dir, name: "v1")
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: true,
            transport: FakeTransport(manifests: { manifest(version: "1.0.0", sha: "aaa") }, archives: { _ in zip }, downloads: Counter()),
            validator: { _ in }
        )

        await store.ensure()

        #expect(await store.state == .ready(version: "1.0.0"))
        let live = try #require(await store.liveBundle)
        #expect(FileManager.default.fileExists(atPath: live.appendingPathComponent("manifest.json").path))
        #expect(FileManager.default.fileExists(atPath: live.appendingPathComponent("model.mlpackage").path))
    }

    @Test("the state stream carries the download and the version it lands on")
    func stateChangesStream() async throws {
        let dir = scratch()
        let zip = try makeBundleZip(in: dir, name: "v1")
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: true,
            transport: FakeTransport(manifests: { manifest(version: "1.0.0", sha: "aaa") }, archives: { _ in zip }, downloads: Counter()),
            validator: { _ in }
        )

        let changes = store.stateChanges
        let collected = Task {
            var seen: [NluBundleState] = []
            for await state in changes {
                seen.append(state)
                if case .ready = state { break }
            }
            return seen
        }

        await store.ensure()
        let states = await collected.value

        #expect(states.first == .downloading(received: 0, total: 1024))
        #expect(states.last == .ready(version: "1.0.0"))
    }

    @Test("an already-installed version is not downloaded again")
    func skipsInstalledVersion() async throws {
        let dir = scratch()
        let zip = try makeBundleZip(in: dir, name: "v1")
        let downloads = Counter()
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: true,
            transport: FakeTransport(manifests: { manifest(version: "1.0.0", sha: "aaa") }, archives: { _ in zip }, downloads: downloads),
            validator: { _ in }
        )

        await store.ensure()
        await store.ensure()

        #expect(downloads.count == 1)
    }

    @Test("a bundle that fails validation leaves the previous one serving")
    func failedValidationKeepsPrevious() async throws {
        let dir = scratch()
        let first = try makeBundleZip(in: dir, name: "v1")
        let second = try makeBundleZip(in: dir, name: "v2")
        let version = Mutable("1.0.0")
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: true,
            transport: FakeTransport(
                manifests: { manifest(version: version.value, sha: version.value) },
                archives: { sha in sha == "1.0.0" ? first : second },
                downloads: Counter()
            ),
            validator: { staged in
                guard version.value == "1.0.0" else { throw NluBundleStoreError.malformedArchive(missing: "weights") }
                _ = staged
            }
        )

        await store.ensure()
        #expect(await store.state == .ready(version: "1.0.0"))

        version.value = "2.0.0"
        await store.ensure()

        #expect(await store.state == .ready(version: "1.0.0"))
        let live = try #require(await store.liveBundle)
        #expect(live.lastPathComponent == "1.0.0")
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("bridgething-nlu/2.0.0").path))
    }

    @Test("an archive missing a required entry never rotates in")
    func malformedArchiveRejected() async throws {
        let dir = scratch()
        let zip = try makeBundleZip(in: dir, name: "v1", entries: ["manifest.json"])
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: true,
            transport: FakeTransport(manifests: { manifest(version: "1.0.0", sha: "aaa") }, archives: { _ in zip }, downloads: Counter()),
            validator: { _ in }
        )

        await store.ensure()

        #expect(await store.liveBundle == nil)
        if case .failed = await store.state {} else {
            Issue.record("expected a failed state, got \(await store.state)")
        }
    }

    @Test("turning the capability off deletes the stored bundle")
    func toggleOffDeletes() async throws {
        let dir = scratch()
        let zip = try makeBundleZip(in: dir, name: "v1")
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: true,
            transport: FakeTransport(manifests: { manifest(version: "1.0.0", sha: "aaa") }, archives: { _ in zip }, downloads: Counter()),
            validator: { _ in }
        )
        await store.ensure()

        await store.setEnabled(false)

        #expect(await store.state == .absent)
        #expect(await store.liveBundle == nil)
        #expect(!FileManager.default.fileExists(atPath: dir.appendingPathComponent("bridgething-nlu").path))
    }

    @Test("a disabled store never checks")
    func disabledStoreSkipsCheck() async throws {
        let dir = scratch()
        let zip = try makeBundleZip(in: dir, name: "v1")
        let downloads = Counter()
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: false,
            transport: FakeTransport(manifests: { manifest(version: "1.0.0", sha: "aaa") }, archives: { _ in zip }, downloads: downloads),
            validator: { _ in }
        )

        await store.ensure()

        #expect(downloads.count == 0)
        #expect(await store.state == .absent)
    }

    @Test("rotating a new version prunes the one it replaced")
    func prunesSuperseded() async throws {
        let dir = scratch()
        let first = try makeBundleZip(in: dir, name: "v1")
        let second = try makeBundleZip(in: dir, name: "v2")
        let version = Mutable("1.0.0")
        let store = NluBundleStore(
            config: .init(storageDirectory: dir),
            enabled: true,
            transport: FakeTransport(
                manifests: { manifest(version: version.value, sha: version.value) },
                archives: { sha in sha == "1.0.0" ? first : second },
                downloads: Counter()
            ),
            validator: { _ in }
        )

        await store.ensure()
        version.value = "2.0.0"
        await store.ensure()

        #expect(await store.state == .ready(version: "2.0.0"))
        let root = dir.appendingPathComponent("bridgething-nlu")
        let entries = Set(try FileManager.default.contentsOfDirectory(atPath: root.path))
        #expect(entries == ["2.0.0", "current"])
    }

    @Test("a new store adopts a bundle already on disk")
    func adoptsExistingInstall() async throws {
        let dir = scratch()
        let zip = try makeBundleZip(in: dir, name: "v1")
        let transport = FakeTransport(
            manifests: { manifest(version: "1.0.0", sha: "aaa") },
            archives: { _ in zip },
            downloads: Counter()
        )
        let first = NluBundleStore(
            config: .init(storageDirectory: dir), enabled: true, transport: transport, validator: { _ in }
        )
        await first.ensure()

        let second = NluBundleStore(
            config: .init(storageDirectory: dir), enabled: true, transport: transport, validator: { _ in }
        )

        #expect(await second.state == .ready(version: "1.0.0"))
        #expect(await second.liveBundle != nil)
    }
}

private final class Mutable<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: T

    init(_ value: T) { stored = value }

    var value: T {
        get {
            lock.lock()
            defer { lock.unlock() }
            return stored
        }
        set {
            lock.lock()
            defer { lock.unlock() }
            stored = newValue
        }
    }
}
