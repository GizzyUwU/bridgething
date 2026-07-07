import Foundation

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
    public let builtinWebapps: [String: String]

    private enum CodingKeys: String, CodingKey {
        case version
        case channel
        case yanked
        case deprecated
        case builtinWebapps = "builtin_webapps"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        version = try container.decode(String.self, forKey: .version)
        channel = try container.decode(String.self, forKey: .channel)
        yanked = try container.decodeIfPresent(String.self, forKey: .yanked)
        deprecated = try container.decode(Bool.self, forKey: .deprecated)
        builtinWebapps = try container.decodeIfPresent([String: String].self, forKey: .builtinWebapps) ?? [:]
    }
}

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

    public static func builtinWebapp(rootURL: URL, channel: String, name: String, version: String) -> URL {
        rootURL
            .appendingPathComponent("webapps")
            .appendingPathComponent(channel)
            .appendingPathComponent(name)
            .appendingPathComponent(version)
            .appendingPathComponent("\(name).zip")
    }
}
