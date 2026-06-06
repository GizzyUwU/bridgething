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
                // clear BOTH stores: ours and spotiny's own keychain, which it reads as a fallback.
                signOut: {
                    spotifyTokenStore.clear()
                    SpotinyClient.eraseTokens()
                },
                hasCredentials: { spotifyTokenStore.load().refresh?.isEmpty == false }
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

    static let spotifyProviderId = SpotifyGlue.name

    private static let spotifyScopes: [String] = [
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

    private static var pkceClientID: String {
        (Bundle.main.object(forInfoDictionaryKey: "BRIDGETHING_PKCE_CLIENT_ID") as? String) ?? ""
    }

    static func spotifyAuthConfig() -> BridgethingSpotifyAuthConfig {
        BridgethingSpotifyAuthConfig(
            scopes: spotifyScopes,
            pkceClientId: pkceClientID,
            pkceRedirectUri: "https://discord.com/api/connections/spotify/callback",
            pkceAuthorizeUrl: "https://accounts.spotify.com/authorize",
            pkceTokenUrl: "https://accounts.spotify.com/api/token"
        )
    }

    static func persistSpotifyTokens(access: String, refresh: String) {
        spotifyTokenStore.save(access: access, refresh: refresh)
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
        let configuration = OAuthConfiguration(
            authorizationEndpoint: URL(string: "https://accounts.spotify.com/authorize")!,
            tokenEndpoint: URL(string: "https://accounts.spotify.com/api/token")!,
            clientID: pkceClientID,
            redirectURI: "https://discord.com/api/connections/spotify/callback",
            scopes: spotifyScopes
        )
        return { WebViewPKCEAuthenticator(configuration: configuration) }
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
        Tokens(
            access: read(account: "access"),
            refresh: read(account: "refresh")
        )
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
