import Foundation

/// Subset of the thinglabs discover-manifest schema needed to pick an
/// OTA release. We only decode the fields the poll loop reads; the
/// site-side validator owns the full schema.
public struct OtaDiscoverManifest: Decodable, Sendable, Equatable {
    public let manifestVersion: Int
    public let updatedAt: String
    public let channels: [String: OtaManifestChannel]
    public let releases: [String: OtaManifestRelease]

    private enum CodingKeys: String, CodingKey {
        case manifestVersion = "manifest_version"
        case updatedAt = "updated_at"
        case channels
        case releases
    }
}

public struct OtaManifestChannel: Decodable, Sendable, Equatable {
    public let name: String
    public let stability: String
    public let isDefault: Bool
    public let latest: String
    public let releases: [String]

    private enum CodingKeys: String, CodingKey {
        case name
        case stability
        case isDefault = "default"
        case latest
        case releases
    }
}

public struct OtaManifestRelease: Decodable, Sendable, Equatable {
    public let version: String
    public let channel: String
    public let yanked: String?
    public let deprecated: Bool
}

/// Composite version parsed out of a channel's `latest`. The bridgething
/// release pipeline uses `<daemon>+image.<image>` (daemon as semver,
/// image as CalVer) so the companion can independently compare each
/// component to the running device's announced versions.
public struct OtaCompositeVersion: Sendable, Equatable {
    public let daemon: String
    public let image: String

    public init(daemon: String, image: String) {
        self.daemon = daemon
        self.image = image
    }

    public static func parse(_ raw: String) -> OtaCompositeVersion? {
        guard let plus = raw.firstIndex(of: "+") else { return nil }
        let daemon = String(raw[..<plus])
        let suffix = raw[raw.index(after: plus)...]
        let prefix = "image."
        guard suffix.hasPrefix(prefix) else { return nil }
        let image = String(suffix.dropFirst(prefix.count))
        if daemon.isEmpty || image.isEmpty { return nil }
        return OtaCompositeVersion(daemon: daemon, image: image)
    }
}

/// Per-artifact URLs derived from the OTA root + channel + per-component
/// version + image variant, matching the on-disk R2 layout. The daemon
/// binary is the only daemon-kind artifact; images carry both .swu (full)
/// and .zck (delta source the daemon's range proxy reads from).
public struct OtaArtifactURLs: Sendable, Equatable {
    public let daemonBinary: URL
    public let imageSwu: URL
    public let imageZck: URL

    public init(
        rootURL: URL,
        channel: String,
        daemonVersion: String,
        imageVersion: String,
        imageVariant: String
    ) {
        let imageName = "bridgething-\(imageVariant)-image"
        daemonBinary = rootURL
            .appendingPathComponent("daemon")
            .appendingPathComponent(channel)
            .appendingPathComponent(daemonVersion)
            .appendingPathComponent("bridgething")
        imageSwu = rootURL
            .appendingPathComponent("images")
            .appendingPathComponent(channel)
            .appendingPathComponent(imageVersion)
            .appendingPathComponent("\(imageName).swu")
        imageZck = rootURL
            .appendingPathComponent("images")
            .appendingPathComponent(channel)
            .appendingPathComponent(imageVersion)
            .appendingPathComponent("\(imageName).zck")
    }
}
