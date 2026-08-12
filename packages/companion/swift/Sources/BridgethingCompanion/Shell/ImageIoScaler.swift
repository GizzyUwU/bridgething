#if canImport(ImageIO) && canImport(CoreGraphics) && canImport(UniformTypeIdentifiers)

    import BridgethingCompanionCore
    import CoreGraphics
    import Foundation
    import ImageIO
    import UniformTypeIdentifiers

    public final class ImageIoScaler: ImageScaler, @unchecked Sendable {
        public init() {}

        public func downsampleJpeg(bytes: Data, maxEdge: UInt32, quality: Float) -> Data? {
            guard let source = CGImageSourceCreateWithData(bytes as CFData, nil) else { return nil }
            let thumbOptions: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: Int(maxEdge),
            ]
            guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, thumbOptions as CFDictionary) else {
                return nil
            }
            let out = NSMutableData()
            guard let dest = CGImageDestinationCreateWithData(out, UTType.jpeg.identifier as CFString, 1, nil) else {
                return nil
            }
            let jpegOptions: [CFString: Any] = [
                kCGImageDestinationLossyCompressionQuality: Double(min(max(quality, 0), 1)),
            ]
            CGImageDestinationAddImage(dest, image, jpegOptions as CFDictionary)
            guard CGImageDestinationFinalize(dest) else { return nil }
            return out as Data
        }
    }

#endif
