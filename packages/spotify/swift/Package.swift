// swift-tools-version: 6.3
import PackageDescription

let package = Package(
  name: "BridgethingSpotify",
  platforms: [
    .iOS(.v18),
    .macOS(.v15),
  ],
  products: [
    .library(name: "BridgethingSpotifyGlue", targets: ["BridgethingSpotifyGlue"]),
  ],
  dependencies: [
    .package(name: "Bridgething", path: "../../.."),
  ],
  targets: [
    .binaryTarget(
      name: "SpotifyFFI",
      path: "Frameworks/SpotifyFFI.xcframework"
    ),
    .target(
      name: "Spotify",
      dependencies: ["SpotifyFFI"],
      path: "Sources/Spotify",
      linkerSettings: [
        .linkedFramework("SystemConfiguration"),
        .linkedFramework("Security"),
        .linkedFramework("CoreFoundation"),
        .unsafeFlags(["-Xlinker", "-no_compact_unwind"]),
      ]
    ),
    .target(
      name: "BridgethingSpotifyGlue",
      dependencies: [
        .product(name: "BridgethingGlue", package: "Bridgething"),
        .product(name: "BridgethingGateway", package: "Bridgething"),
        .product(name: "BridgethingSchema", package: "Bridgething"),
        "Spotify",
      ],
      path: "Sources/BridgethingSpotifyGlue"
    ),
    .testTarget(
      name: "BridgethingSpotifyGlueTests",
      dependencies: [
        "BridgethingSpotifyGlue",
        "Spotify",
        .product(name: "BridgethingCompanion", package: "Bridgething"),
        .product(name: "BridgethingTestKit", package: "Bridgething"),
        .product(name: "BridgethingGateway", package: "Bridgething"),
        .product(name: "BridgethingSchema", package: "Bridgething"),
        .product(name: "BridgethingGlue", package: "Bridgething"),
      ],
      path: "Tests/BridgethingSpotifyGlueTests"
    ),
  ]
)
