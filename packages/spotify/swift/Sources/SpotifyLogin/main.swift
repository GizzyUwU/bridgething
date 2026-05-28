import BridgethingTestKit
import CryptoKit
import Foundation
import Spotiny

let scopes = [
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
    "app-remote-control",
]

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("\(message)\n".utf8))
    exit(1)
}

// MARK: - PKCE flow

func base64URL(_ bytes: some Sequence<UInt8>) -> String {
    Data(bytes).base64EncodedString()
        .replacingOccurrences(of: "+", with: "-")
        .replacingOccurrences(of: "/", with: "_")
        .replacingOccurrences(of: "=", with: "")
}

func randomURLSafe(byteCount: Int) -> String {
    var bytes = [UInt8]()
    var rng = SystemRandomNumberGenerator()
    for _ in 0 ..< byteCount { bytes.append(UInt8.random(in: 0 ... 255, using: &rng)) }
    return base64URL(bytes)
}

func extractCode(from input: String) -> String? {
    let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
    if let comps = URLComponents(string: trimmed),
       let code = comps.queryItems?.first(where: { $0.name == "code" })?.value {
        return code
    }
    if let range = trimmed.range(of: "code=") {
        let tail = trimmed[range.upperBound...]
        return String(tail.prefix { $0 != "&" })
    }
    return trimmed.isEmpty ? nil : trimmed
}

func runPKCELogin() async {
    let env = ProcessInfo.processInfo.environment
    guard let clientID = env["BRIDGETHING_PKCE_CLIENT_ID"], !clientID.isEmpty else {
        fail("set BRIDGETHING_PKCE_CLIENT_ID")
    }
    guard let redirectURI = env["BRIDGETHING_PKCE_REDIRECT"], !redirectURI.isEmpty else {
        fail("set BRIDGETHING_PKCE_REDIRECT to the client's registered OAuth redirect URI")
    }

    let verifier = randomURLSafe(byteCount: 64)
    let challenge = base64URL(SHA256.hash(data: Data(verifier.utf8)))
    let state = randomURLSafe(byteCount: 16)

    guard var authComponents = URLComponents(string: "https://accounts.spotify.com/authorize") else {
        fail("could not build authorize URL")
    }
    authComponents.queryItems = [
        URLQueryItem(name: "client_id", value: clientID),
        URLQueryItem(name: "response_type", value: "code"),
        URLQueryItem(name: "redirect_uri", value: redirectURI),
        URLQueryItem(name: "code_challenge_method", value: "S256"),
        URLQueryItem(name: "code_challenge", value: challenge),
        URLQueryItem(name: "scope", value: scopes.joined(separator: " ")),
        URLQueryItem(name: "state", value: state),
    ]
    guard let authURL = authComponents.url else { fail("could not build authorize URL") }

    print("""

      PKCE login (client \(clientID))

      1. Open this URL, log in, and approve:

         \(authURL.absoluteString)

      2. You'll be redirected to \(redirectURI)?code=...
         The page may show an error - that's expected. Copy the FULL redirected
         URL from the address bar (or just the code= value).

      3. Paste it here and press Enter:
      """)

    guard let line = readLine(strippingNewline: true), let code = extractCode(from: line) else {
        fail("no authorization code provided")
    }

    guard let tokenURL = URL(string: "https://accounts.spotify.com/api/token") else {
        fail("bad token endpoint")
    }
    var request = URLRequest(url: tokenURL)
    request.httpMethod = "POST"
    request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
    var form = URLComponents()
    form.queryItems = [
        URLQueryItem(name: "grant_type", value: "authorization_code"),
        URLQueryItem(name: "code", value: code),
        URLQueryItem(name: "redirect_uri", value: redirectURI),
        URLQueryItem(name: "client_id", value: clientID),
        URLQueryItem(name: "code_verifier", value: verifier),
    ]
    request.httpBody = form.percentEncodedQuery.map { Data($0.utf8) }

    struct TokenResponse: Decodable {
        let access_token: String
        let refresh_token: String?
    }

    do {
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? -1
        guard (200 ..< 300).contains(status) else {
            fail("token endpoint \(status): \(String(data: data, encoding: .utf8) ?? "")")
        }
        let token = try JSONDecoder().decode(TokenResponse.self, from: data)
        guard !token.access_token.isEmpty else { fail("token endpoint returned an empty access token") }
        try SpotifyTokenStore.save(SpotifyTokens(accessToken: token.access_token, refreshToken: token.refresh_token ?? ""))
        print("Saved PKCE tokens to \(SpotifyTokenStore.path().path)")
        if token.refresh_token == nil {
            print("note: no refresh token returned; tests will work until the access token expires.")
        }
    } catch {
        fail("PKCE token exchange failed: \(error)")
    }
}

// MARK: - Device-code flow

func runDeviceCodeLogin() async {
    // Route through the auth Worker, exactly as the app does: the Worker injects
    // the client_id server-side, so this only carries the PSK. The minted token is
    // bound to the Worker's client_id, so the Worker can later refresh it.
    guard let psk = ProcessInfo.processInfo.environment["BRIDGETHING_AUTH_PSK"], !psk.isEmpty else {
        fail("set BRIDGETHING_AUTH_PSK")
    }
    let worker = ProcessInfo.processInfo.environment["BRIDGETHING_AUTH_WORKER"] ?? "https://thinglabs.sh/auth"

    let config = DeviceCodeConfiguration(
        deviceCodeEndpoint: URL(string: "\(worker)/api/device/code")!,
        tokenEndpoint: URL(string: "\(worker)/api/token")!,
        description: "bridgething-test",
        scopes: scopes,
        authorizationBearer: psk
    )

    let authenticator = DeviceCodeAuthenticator(configuration: config) { prompt in
        print("")
        print("  Open: \(prompt.verificationURLPrefilled.absoluteString)")
        print("  (or visit \(prompt.verificationURL.absoluteString) and enter code: \(prompt.userCode))")
        print("  Waiting for you to approve in the browser...")
        print("")
    }

    do {
        let token = try await authenticator.authorize()
        let refresh = token.refreshToken ?? ""
        guard !token.accessToken.isEmpty else {
            fail("login returned an empty access token")
        }
        try SpotifyTokenStore.save(SpotifyTokens(accessToken: token.accessToken, refreshToken: refresh))
        print("Saved Spotify tokens to \(SpotifyTokenStore.path().path)")
        if refresh.isEmpty {
            print("note: no refresh token returned; tests will work until the access token expires.")
        }
    } catch {
        fail("login failed: \(error)")
    }
}

if CommandLine.arguments.dropFirst().contains("pkce") {
    await runPKCELogin()
} else {
    await runDeviceCodeLogin()
}
