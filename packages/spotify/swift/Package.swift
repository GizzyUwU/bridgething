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
    .package(name: "Spotiny", path: "spotiny"),
  ],
  targets: [
    .target(
      name: "BridgethingSpotifyGlue",
      dependencies: [
        .product(name: "BridgethingGlue", package: "Bridgething"),
        .product(name: "BridgethingGateway", package: "Bridgething"),
        .product(name: "BridgethingSchema", package: "Bridgething"),
        .product(name: "Spotiny", package: "Spotiny"),
      ],
      path: "Sources/BridgethingSpotifyGlue"
    ),
    .executableTarget(
      name: "spotify-login",
      dependencies: [
        .product(name: "Spotiny", package: "Spotiny"),
        .product(name: "BridgethingTestKit", package: "Bridgething"),
      ],
      path: "Sources/SpotifyLogin"
    ),
    .testTarget(
      name: "BridgethingSpotifyGlueTests",
      dependencies: [
        "BridgethingSpotifyGlue",
        .product(name: "Spotiny", package: "Spotiny"),
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
