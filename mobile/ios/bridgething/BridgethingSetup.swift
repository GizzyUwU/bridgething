import BridgethingAppleMusicGlue
import BridgethingCompanion
import BridgethingLyrics
import BridgethingSession
import BridgethingSpotifyGlue
import Foundation

enum BridgethingApp {
    static let appName: String = "bridgething"
    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
    }

    private static let spotifyWorkerBase = "https://thinglabs.sh/auth"
    private static let spotifyTokenStore = SpotifyKeychainStore(service: "com.bridgething.spotify")

    static func installBridgething() {
        HybridBridgethingSessionImpl.hostInfo = HostInfo(
            appName: appName,
            appVersion: appVersion,
            osName: "iOS"
        )
        HybridBridgethingSessionImpl.lyricsResolver = LrclibResolver()
        HybridBridgethingSessionImpl.eaProtocolString = "com.bridgething.gateway"

        HybridBridgethingSessionImpl.registry = [
            HybridBridgethingSessionImpl.ProviderRegistration(
                id: SpotifyGlue.name,
                displayName: SpotifyGlue.displayName,
                available: true,
                factory: { makeSpotifyGlue() },
                signOut: { spotifyTokenStore.clear() },
                hasCredentials: { spotifyTokenStore.loadRefreshToken() != nil }
            ),
            HybridBridgethingSessionImpl.ProviderRegistration(
                id: AppleMusicGlue.name,
                displayName: AppleMusicGlue.displayName,
                available: false,
                factory: { AppleMusicGlue() },
                signOut: {}
            ),
        ]

        HybridBridgethingSession.installBackend(HybridBridgethingSessionImpl())
    }

    static let spotifyProviderId = SpotifyGlue.name

    private static var authPsk: String {
        (Bundle.main.object(forInfoDictionaryKey: "BRIDGETHING_AUTH_PSK") as? String) ?? ""
    }

    private static func makeSpotifyGlue() -> SpotifyGlue {
        SpotifyGlue(
            workerBase: spotifyWorkerBase,
            psk: authPsk,
            deviceId: spotifyTokenStore.deviceId(),
            tokenStore: spotifyTokenStore
        )
    }
}

private final class SpotifyKeychainStore: Spotify.TokenStore, @unchecked Sendable {
    private let service: String

    init(service: String) {
        self.service = service
    }

    func loadRefreshToken() -> String? { read(account: "refresh") }
    func saveRefreshToken(token: String) { write(account: "refresh", value: token) }
    func loadUsername() -> String? { read(account: "username") }
    func saveUsername(username: String) { write(account: "username", value: username) }

    func clear() {
        delete(account: "refresh")
        delete(account: "username")
    }

    func deviceId() -> String {
        if let existing = read(account: "device_id") { return existing }
        var bytes = [UInt8](repeating: 0, count: 20)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        let id = bytes.map { String(format: "%02x", $0) }.joined()
        write(account: "device_id", value: id)
        return id
    }

    private func read(account: String) -> String? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecMatchLimit as String: kSecMatchLimitOne,
            kSecReturnData as String: true,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &item)
        if status != errSecSuccess {
            if status != errSecItemNotFound {
                NSLog("bridgething: keychain read failed for \(account): \(status)")
            }
            return nil
        }
        guard let data = item as? Data, let value = String(data: data, encoding: .utf8) else { return nil }
        return value
    }

    private func write(account: String, value: String) {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let attrs: [String: Any] = [
            kSecValueData as String: Data(value.utf8),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        var status = SecItemUpdate(q as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound {
            var insert = q
            insert.merge(attrs) { _, b in b }
            status = SecItemAdd(insert as CFDictionary, nil)
        }
        if status != errSecSuccess {
            NSLog("bridgething: keychain write failed for \(account): \(status)")
        }
    }

    private func delete(account: String) {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(q as CFDictionary)
    }
}
