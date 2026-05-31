import Foundation

/// A `catalog.v1` app catalog as served at a source URL
public struct Catalog: Codable, Sendable, Equatable {
    public let schema: String
    public let updatedAt: String
    public let repo: CatalogRepo
    public let apps: [CatalogApp]
    public let recommendedSources: [CatalogRecommendedSource]

    private enum CodingKeys: String, CodingKey {
        case schema
        case updatedAt = "updated_at"
        case repo
        case apps
        case recommendedSources = "recommended_sources"
    }
}

public struct CatalogRepo: Codable, Sendable, Equatable {
    public let name: String
    public let description: String
    public let homepage: String?
    public let icon: String?
}

public struct CatalogApp: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let description: String
    public let author: String
    public let icon: String?
    public let homepage: String?
    public let source: String?
    public let versions: [CatalogAppVersion]
}

public struct CatalogAppVersion: Codable, Sendable, Equatable {
    public let version: String
    public let releasedAt: String
    public let download: CatalogDownload
    public let permissions: [String]
    public let minLibbridgethingVersion: String
    public let changelog: String?

    private enum CodingKeys: String, CodingKey {
        case version
        case releasedAt = "released_at"
        case download
        case permissions
        case minLibbridgethingVersion = "min_libbridgething_version"
        case changelog
    }
}

public struct CatalogDownload: Codable, Sendable, Equatable {
    public let url: String
    public let size: Int
    public let sha256: String
}

public struct CatalogRecommendedSource: Codable, Sendable, Equatable {
    public let name: String
    public let url: String
    public let description: String?
    public let attested: Bool
}

public enum SemverCompat {
    public static func satisfies(deviceVersion: String, minimum: String) -> Bool {
        compare(deviceVersion, minimum) >= 0
    }

    static func compare(_ a: String, _ b: String) -> Int {
        let pa = components(a)
        let pb = components(b)
        for i in 0 ..< max(pa.count, pb.count) {
            let x = i < pa.count ? pa[i] : 0
            let y = i < pb.count ? pb[i] : 0
            if x != y { return x < y ? -1 : 1 }
        }
        return 0
    }

    static func components(_ s: String) -> [Int] {
        var v = Substring(s)
        if v.first == "v" || v.first == "V" { v = v.dropFirst() }
        if let cut = v.firstIndex(where: { $0 == "-" || $0 == "+" }) { v = v[..<cut] }
        return v.split(separator: ".").map { Int($0) ?? 0 }
    }
}
