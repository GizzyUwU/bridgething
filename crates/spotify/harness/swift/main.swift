import Foundation

final class FileStore: TokenStore, @unchecked Sendable {
    private let dir: String
    init(dir: String) {
        self.dir = dir
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    }
    private func read(_ name: String) -> String? {
        let p = (dir as NSString).appendingPathComponent(name)
        guard let s = try? String(contentsOfFile: p, encoding: .utf8) else { return nil }
        let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
        return t.isEmpty ? nil : t
    }
    private func write(_ name: String, _ value: String) {
        let p = (dir as NSString).appendingPathComponent(name)
        try? value.write(toFile: p, atomically: true, encoding: .utf8)
    }
    func loadRefreshToken() -> String? { read(".refresh_token.txt") }
    func saveRefreshToken(token: String) { write(".refresh_token.txt", token) }
    func loadUsername() -> String? { read(".username") }
    func saveUsername(username: String) { write(".username", username) }
}

final class Printer: Observer, @unchecked Sendable {
    func onPlayer(state: PlayerState) {
        let track: String
        if let t = state.track {
            track = "\(t.name) - \(t.artists.map { $0.name }.joined(separator: ", "))"
        } else {
            track = "(nothing)"
        }
        print("[swift player] \(track) | \(state.isPaused ? "paused" : "playing") | \(state.positionMs / 1000)s/\(state.durationMs / 1000)s")
    }
    func onQueue(queue: Queue) { print("[swift queue] \(queue.next.count) upcoming") }
    func onDevices(devices: [Device]) {
        print("[swift devices] \(devices.map { $0.name }.joined(separator: ", "))")
    }
    func onAuth(state: AuthState) { print("[swift auth] \(state)") }
    func onLibraryChanged(scope: LibraryScope) { print("[swift library] changed: \(scope)") }
}

let env = ProcessInfo.processInfo.environment
guard let psk = env["SPOTIFY_AUTH_PSK"] else {
    FileHandle.standardError.write("need SPOTIFY_AUTH_PSK\n".data(using: .utf8)!)
    exit(1)
}
let state = env["SPOTIFY_PRIVATE_STATE"] ?? "/tmp/sfp-live"
let deviceId = env["SPOTIFY_DEVICE_ID"] ?? "00112233445566778899aabbccddeeff00112233"

let store = FileStore(dir: state)
if store.loadRefreshToken() == nil, let seed = env["SPOTIFY_CARTHING_REFRESH_TOKEN"], !seed.isEmpty {
    store.saveRefreshToken(token: seed)
}

let client = SpotifyClient.create(
    base: "https://thinglabs.sh/auth",
    psk: psk,
    deviceId: deviceId,
    store: store,
    observer: Printer()
)

let done = DispatchSemaphore(value: 0)
Task {
    do {
        try await client.connect()
        print("[swift] connected")
    } catch {
        print("[swift] connect error: \(error)")
    }
    done.signal()
}
done.wait()
print("[swift] watching 12s...")
Thread.sleep(forTimeInterval: 12)
print("[swift] done")
