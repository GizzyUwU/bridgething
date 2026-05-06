// swift-tools-version: 6.3
import PackageDescription

let package = Package(
  name: "BridgethingAppleMusic",
  platforms: [
    .iOS(.v18),
    .macOS(.v15),
  ],
  products: [
    .library(name: "BridgethingAppleMusicGlue", targets: ["BridgethingAppleMusicGlue"]),
  ],
  dependencies: [
    .package(name: "Bridgething", path: "../../.."),
  ],
  targets: [
    .target(
      name: "BridgethingAppleMusicGlue",
      dependencies: [
        .product(name: "BridgethingGlue", package: "Bridgething"),
        .product(name: "BridgethingGateway", package: "Bridgething"),
        .product(name: "BridgethingSchema", package: "Bridgething"),
      ],
      path: "Sources/BridgethingAppleMusicGlue"
    ),
  ]
)
