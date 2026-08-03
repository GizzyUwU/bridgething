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
    .library(name: "BridgethingNluKit", targets: ["BridgethingNluKit"]),
    .library(name: "BridgethingAppleMusicGlue", targets: ["BridgethingAppleMusicGlue"]),
    .library(name: "BridgethingTestKit", targets: ["BridgethingTestKit"]),
  ],
  dependencies: [
    .package(url: "https://github.com/fumoboy007/msgpack-swift", from: "2.0.6"),
    .package(url: "https://github.com/1024jp/GzipSwift", from: "6.1.0"),
    .package(url: "https://github.com/apple/swift-log", from: "1.6.0"),
    .package(url: "https://github.com/weichsel/ZIPFoundation.git", from: "0.9.20"),
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
        .product(name: "Logging", package: "swift-log"),
      ],
      path: "packages/gateway/swift/Sources/BridgethingGateway",
      linkerSettings: [
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
      dependencies: [
        "BridgethingGateway", "BridgethingGlue", "BridgethingLyrics", "BridgethingSchema",
        .product(name: "Logging", package: "swift-log"),
        .product(name: "ZIPFoundation", package: "ZIPFoundation"),
      ],
      path: "packages/companion/swift/Sources/BridgethingCompanion",
      resources: [.process("Resources")]
    ),
    .target(
      name: "BridgethingTestKit",
      dependencies: ["BridgethingGateway", "BridgethingSchema", "BridgethingGlue", "BridgethingLyrics"],
      path: "packages/testkit/swift/Sources/BridgethingTestKit"
    ),
    .testTarget(
      name: "BridgethingCompanionTests",
      dependencies: ["BridgethingCompanion", "BridgethingTestKit",.product(name: "ZIPFoundation", package: "ZIPFoundation"),],
      path: "packages/companion/swift/Tests/BridgethingCompanionTests"
    ),
    .binaryTarget(
      name: "NluFFI",
      path: "packages/nlu/swift/Frameworks/NluFFI.xcframework"
    ),
    .target(
      name: "Nlu",
      dependencies: ["NluFFI"],
      path: "packages/nlu/swift/Sources/Nlu"
    ),
    .target(
      name: "BridgethingNluKit",
      dependencies: ["Nlu", "BridgethingCompanion", "BridgethingSchema"],
      path: "packages/nlu/swift/Sources/BridgethingNluKit"
    ),
    .testTarget(
      name: "BridgethingNluKitTests",
      dependencies: ["BridgethingNluKit", "Nlu"],
      path: "packages/nlu/swift/Tests/BridgethingNluKitTests"
    ),
    .target(
      name: "BridgethingAppleMusicGlue",
      dependencies: ["BridgethingGlue", "BridgethingGateway", "BridgethingSchema"],
      path: "packages/apple-music/swift/Sources/BridgethingAppleMusicGlue"
    ),
    .testTarget(
      name: "BridgethingAppleMusicGlueTests",
      dependencies: ["BridgethingAppleMusicGlue", "BridgethingCompanion", "BridgethingTestKit"],
      path: "packages/apple-music/swift/Tests/BridgethingAppleMusicGlueTests"
    ),
  ]
)
