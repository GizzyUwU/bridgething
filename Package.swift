// swift-tools-version: 6.3
import PackageDescription

let package = Package(
  name: "Bridgething",
  platforms: [
    .iOS(.v18),
    .macOS(.v15),
  ],
  products: [
    .library(name: "BridgethingSchema", targets: ["BridgethingSchema"]),
    .library(name: "BridgethingGateway", targets: ["BridgethingGateway"]),
    .library(name: "BridgethingLyrics", targets: ["BridgethingLyrics"]),
    .library(name: "BridgethingGlue", targets: ["BridgethingGlue"]),
    .library(name: "BridgethingCompanion", targets: ["BridgethingCompanion"]),
  ],
  dependencies: [
    .package(url: "https://github.com/fumoboy007/msgpack-swift", from: "2.0.6"),
    .package(url: "https://github.com/1024jp/GzipSwift", from: "6.1.0"),
  ],
  targets: [
    .target(
      name: "BridgethingSchema",
      path: "crates/lib/swift/Sources/BridgethingSchema"
    ),
    .target(
      name: "BridgethingGateway",
      dependencies: [
        "BridgethingSchema",
        .product(name: "DMMessagePack", package: "msgpack-swift"),
        .product(name: "Gzip", package: "GzipSwift"),
      ],
      path: "packages/gateway/swift/Sources/BridgethingGateway"
    ),
    .testTarget(
      name: "BridgethingGatewayTests",
      dependencies: ["BridgethingGateway"],
      path: "packages/gateway/swift/Tests/BridgethingGatewayTests"
    ),
    .target(
      name: "BridgethingLyrics",
      path: "packages/lyrics/swift/Sources/BridgethingLyrics"
    ),
    .testTarget(
      name: "BridgethingLyricsTests",
      dependencies: ["BridgethingLyrics"],
      path: "packages/lyrics/swift/Tests/BridgethingLyricsTests"
    ),
    .target(
      name: "BridgethingGlue",
      dependencies: ["BridgethingGateway", "BridgethingLyrics"],
      path: "packages/glue/swift/Sources/BridgethingGlue"
    ),
    .target(
      name: "BridgethingCompanion",
      dependencies: ["BridgethingGateway", "BridgethingGlue", "BridgethingLyrics", "BridgethingSchema"],
      path: "packages/companion/swift/Sources/BridgethingCompanion"
    ),
  ]
)
