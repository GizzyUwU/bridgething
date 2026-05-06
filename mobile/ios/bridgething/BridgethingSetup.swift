import BridgethingAppleMusicGlue
import BridgethingCompanion
import BridgethingLyrics
import BridgethingSession
import BridgethingSpotifyGlue
import BridgethingTidalGlue
import Foundation
import Spotiny

/// Wires the bridgething Nitro session module's static registry before
/// React Native starts. AppDelegate calls `installBridgething()` from
/// `application(_:didFinishLaunchingWithOptions:)`.
///
/// The host-app's Info.plist supplies `BRIDGETHING_DEVICE_CLIENT_ID`
/// (gitignored xcconfig); when present the Spotify glue defaults to the
/// device-code authenticator, otherwise to the WebView PKCE flow against
/// a bridgething-owned client (set `BRIDGETHING_PKCE_CLIENT_ID`).
enum Bridgething {
    static let appName: String = "bridgething"
    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
    }

    static func installBridgething() {
        HybridBridgethingSession.hostInfo = HostInfo(
            appName: appName,
            appVersion: appVersion,
            osName: "iOS"
        )
        HybridBridgethingSession.lyricsResolver = LrclibResolver()
        HybridBridgethingSession.eaProtocolString = "com.bridgething.gateway"

        HybridBridgethingSession.registry = [
            HybridBridgethingSession.ProviderRegistration(
                id: SpotifyGlue.name,
                displayName: SpotifyGlue.displayName,
                available: true
            ) { makeSpotifyGlue() },
            HybridBridgethingSession.ProviderRegistration(
                id: AppleMusicGlue.name,
                displayName: AppleMusicGlue.displayName,
                available: false
            ) { AppleMusicGlue() },
            HybridBridgethingSession.ProviderRegistration(
                id: TidalGlue.name,
                displayName: TidalGlue.displayName,
                available: false
            ) { TidalGlue() },
        ]
    }

    private static func makeSpotifyGlue() -> SpotifyGlue {
        let authenticator = makeSpotifyAuthenticator()
        let store = TokenStore(service: "dev.bridgething.spotify")
        let initial = store.load()
        return SpotifyGlue(
            authenticator: authenticator,
            accessToken: initial.access ?? "",
            refreshToken: initial.refresh ?? "",
            onTokensRefreshed: { access, refresh in
                store.save(access: access, refresh: refresh)
            }
        )
    }

    private static func makeSpotifyAuthenticator() -> any OAuthAuthenticator {
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
            let configuration = DeviceCodeConfiguration(
                deviceCodeEndpoint: deviceCodeURL,
                tokenEndpoint: tokenURL,
                clientID: clientID,
                scopes: scopes
            )
            return DeviceCodeAuthenticator(configuration: configuration) { _ in }
        }

        let pkceClientID = (Bundle.main.object(forInfoDictionaryKey: "BRIDGETHING_PKCE_CLIENT_ID") as? String) ?? ""
        let configuration = OAuthConfiguration(
            authorizationEndpoint: authorizeURL,
            tokenEndpoint: tokenURL,
            clientID: pkceClientID,
            redirectURI: "bridgething://oauth",
            scopes: scopes
        )
        return WebViewPKCEAuthenticator(configuration: configuration)
    }
}

/// Keychain-backed token persistence. Spotiny calls `onTokensRefreshed`
/// every time it rotates tokens; the glue's init reads back on next start.
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
}
