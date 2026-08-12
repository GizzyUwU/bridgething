import BridgethingCompanionCore
import BridgethingSession
import Foundation

enum BridgethingApp {
    static let appName: String = "bridgething"
    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
    }

    private static let spotifyWorkerBase = "https://thinglabs.sh/auth"

    static func installBridgething() {
        HybridBridgethingSessionImpl.hostInfo = HostInfo(
            appName: appName,
            appVersion: appVersion,
            osName: "iOS",
            osVersion: "",
            hostIdentifier: ""
        )
        HybridBridgethingSessionImpl.eaProtocolString = "com.bridgething.gateway"
        if let psk = Bundle.main.object(forInfoDictionaryKey: "BRIDGETHING_AUTH_PSK") as? String, !psk.isEmpty {
            HybridBridgethingSessionImpl.spotifyConfig = SpotifyProviderConfig(
                workerBase: spotifyWorkerBase,
                psk: psk
            )
        }

        HybridBridgethingSession.installBackend(HybridBridgethingSessionImpl())
    }
}
