import BridgethingSchema
import Foundation
#if canImport(ImageIO)
    import CoreGraphics
    import ImageIO
#endif

public extension LibraryItem {
    var artworkId: String? {
        switch self {
        case let .track(t): return t.imageId.isEmpty ? nil : t.imageId
        case let .playlist(p): return p.artworkId
        case let .podcastEpisode(e): return e.artworkId
        case let .show(s): return s.artworkId
        case let .station(s): return s.artworkId
        case let .album(a): return a.artworkId
        case let .artist(a): return a.artworkId
        }
    }
}

public struct ImageAssetCodec: Sendable {
    public let namespace: String
    public let shortForm: (tag: Character, urlPrefix: String)?

    public init(namespace: String, shortForm: (tag: Character, urlPrefix: String)? = nil) {
        self.namespace = namespace
        self.shortForm = shortForm
    }

    public func assetId(url rawURL: String, maxEdge: Int) -> String? {
        guard !rawURL.isEmpty else { return nil }
        if let shortForm, rawURL.hasPrefix(shortForm.urlPrefix) {
            return "\(namespace)\(maxEdge)/\(shortForm.tag)\(rawURL.dropFirst(shortForm.urlPrefix.count))"
        }
        guard let encoded = rawURL.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) else { return nil }
        return "\(namespace)\(maxEdge)/u\(encoded)"
    }

    public func parse(_ id: String) -> (url: URL, maxEdge: Int)? {
        guard id.hasPrefix(namespace) else { return nil }
        let rest = id.dropFirst(namespace.count)
        guard let slash = rest.firstIndex(of: "/"), let maxEdge = Int(rest[..<slash]) else { return nil }
        let tagged = rest[rest.index(after: slash)...]
        guard let tag = tagged.first else { return nil }
        let body = String(tagged.dropFirst())
        let urlString: String
        if let shortForm, tag == shortForm.tag {
            urlString = shortForm.urlPrefix + body
        } else if tag == "u" {
            guard let decoded = body.removingPercentEncoding else { return nil }
            urlString = decoded
        } else {
            return nil
        }
        guard let url = URL(string: urlString) else { return nil }
        return (url, maxEdge)
    }
}

public enum ArtImage {
    public static func downsampleJpeg(_ data: Data, maxEdge: Int, quality: Double = 0.6) -> Data? {
        #if canImport(ImageIO)
            guard let src = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
            let opts: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maxEdge,
            ]
            guard let thumb = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
            let out = NSMutableData()
            guard let dest = CGImageDestinationCreateWithData(out as CFMutableData, "public.jpeg" as CFString, 1, nil) else { return nil }
            CGImageDestinationAddImage(dest, thumb, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
            guard CGImageDestinationFinalize(dest) else { return nil }
            return out as Data
        #else
            return nil
        #endif
    }
}
