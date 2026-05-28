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
    .library(name: "BridgethingAppleMusicGlue", targets: ["BridgethingAppleMusicGlue"]),
    .library(name: "BridgethingTidalGlue", targets: ["BridgethingTidalGlue"]),
    .library(name: "BridgethingTestKit", targets: ["BridgethingTestKit"]),
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
      path: "packages/gateway/swift/Sources/BridgethingGateway",
      linkerSettings: [
        // IOBluetoothRFCOMMAdapter only compiles on macOS (gated by
        // canImport(IOBluetooth)); link the framework when building
        // for that platform so the symbols resolve.
        .linkedFramework("IOBluetooth", .when(platforms: [.macOS])),
      ]
    ),
    .testTarget(
      name: "BridgethingGatewayTests",
      dependencies: ["BridgethingGateway", "BridgethingTestKit"],
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
    .target(
      name: "BridgethingTestKit",
      dependencies: ["BridgethingGateway", "BridgethingSchema", "BridgethingGlue", "BridgethingLyrics"],
      path: "packages/testkit/swift/Sources/BridgethingTestKit"
    ),
    .testTarget(
      name: "BridgethingCompanionTests",
      dependencies: ["BridgethingCompanion", "BridgethingTestKit"],
      path: "packages/companion/swift/Tests/BridgethingCompanionTests"
    ),
    .target(
      name: "BridgethingAppleMusicGlue",
      dependencies: ["BridgethingGlue", "BridgethingGateway", "BridgethingSchema"],
      path: "packages/apple-music/swift/Sources/BridgethingAppleMusicGlue"
    ),
    .target(
      name: "BridgethingTidalGlue",
      dependencies: ["BridgethingGlue", "BridgethingGateway", "BridgethingSchema"],
      path: "packages/tidal/swift/Sources/BridgethingTidalGlue"
    ),
  ]
)
