import BridgethingAppleMusicGlue
import BridgethingCompanion
import BridgethingLyrics
import BridgethingSession
import BridgethingSpotifyGlue
import BridgethingTidalGlue
import Foundation
import Spotiny

/// Populates the static provider registry and installs the session backend.
/// Call from `application(_:didFinishLaunchingWithOptions:)` before React Native starts.
/// When `BRIDGETHING_DEVICE_CLIENT_ID` is present in Info.plist, Spotify uses the device-code
/// flow; otherwise falls back to WebView PKCE via `BRIDGETHING_PKCE_CLIENT_ID`.
enum BridgethingApp {
    static let appName: String = "bridgething"
    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
    }

    private static let spotifyTokenStore = TokenStore(service: "dev.bridgething.spotify")

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
                signOut: { spotifyTokenStore.clear() }
            ),
            HybridBridgethingSessionImpl.ProviderRegistration(
                id: AppleMusicGlue.name,
                displayName: AppleMusicGlue.displayName,
                available: false,
                factory: { AppleMusicGlue() },
                signOut: {}
            ),
            HybridBridgethingSessionImpl.ProviderRegistration(
                id: TidalGlue.name,
                displayName: TidalGlue.displayName,
                available: false,
                factory: { TidalGlue() },
                signOut: {}
            ),
        ]

        HybridBridgethingSession.installBackend(HybridBridgethingSessionImpl())
    }

    private static func makeSpotifyGlue() -> SpotifyGlue {
        let initial = spotifyTokenStore.load()
        return SpotifyGlue(
            authenticatorFactory: spotifyAuthenticatorFactory(),
            accessToken: initial.access ?? "",
            refreshToken: initial.refresh ?? "",
            onTokensRefreshed: { access, refresh in
                spotifyTokenStore.save(access: access, refresh: refresh)
            }
        )
    }

    private static func spotifyAuthenticatorFactory() -> SpotifyAuthenticatorFactory {
        let scopes: [String] = [
            "user-read-playback-state",
            "user-modify-playback-state",
            "user-read-currently-playing",
            "user-read-playback-position",
            "user-top-read",
            "user-read-recently-played",
            "playlist-read-private",
            "playlist-read-collaborative",
            "playlist-modify-private",
            "playlist-modify-public",
            "user-follow-modify",
            "user-follow-read",
            "user-library-read",
            "user-library-modify",
            "user-read-private",
        ]
        let authorizeURL = URL(string: "https://accounts.spotify.com/authorize")!
        let tokenURL = URL(string: "https://accounts.spotify.com/api/token")!

        if let clientID = Bundle.main.object(forInfoDictionaryKey: "BRIDGETHING_DEVICE_CLIENT_ID") as? String,
           !clientID.isEmpty
        {
            let deviceCodeURL = URL(string: "https://accounts.spotify.com/api/device/code")!
            // spotify's device-code endpoint requires `description` as the device label shown on spotify.com/pair
            let configuration = DeviceCodeConfiguration(
                deviceCodeEndpoint: deviceCodeURL,
                tokenEndpoint: tokenURL,
                clientID: clientID,
                description: "car-thing-device",
                scopes: scopes
            )
            return { onPrompt in
                DeviceCodeAuthenticator(configuration: configuration, onPrompt: onPrompt)
            }
        }

        let pkceClientID = (Bundle.main.object(forInfoDictionaryKey: "BRIDGETHING_PKCE_CLIENT_ID") as? String) ?? ""
        let configuration = OAuthConfiguration(
            authorizationEndpoint: authorizeURL,
            tokenEndpoint: tokenURL,
            clientID: pkceClientID,
            redirectURI: "bridgething://oauth",
            scopes: scopes
        )
        return { _ in
            // PKCE flow produces no device-code prompt; closure argument is unused
            WebViewPKCEAuthenticator(configuration: configuration)
        }
    }
}

/// Keychain-backed token persistence for Spotify credentials.
private final class TokenStore: @unchecked Sendable {
    private let service: String

    init(service: String) {
        self.service = service
    }

    struct Tokens {
        let access: String?
        let refresh: String?
    }

    func load() -> Tokens {
        Tokens(access: read(account: "access"), refresh: read(account: "refresh"))
    }

    func save(access: String, refresh: String) {
        write(account: "access", value: access)
        write(account: "refresh", value: refresh)
    }

    func clear() {
        delete(account: "access")
        delete(account: "refresh")
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
        guard SecItemCopyMatching(q as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data,
              let value = String(data: data, encoding: .utf8)
        else { return nil }
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
        ]
        let status = SecItemUpdate(q as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound {
            var insert = q
            insert.merge(attrs) { _, b in b }
            SecItemAdd(insert as CFDictionary, nil)
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
