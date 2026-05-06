// swift-tools-version: 6.3
import PackageDescription

let package = Package(
  name: "BridgethingTidal",
  platforms: [
    .iOS(.v18),
    .macOS(.v15),
  ],
  products: [
    .library(name: "BridgethingTidalGlue", targets: ["BridgethingTidalGlue"]),
  ],
  dependencies: [
    .package(name: "Bridgething", path: "../../.."),
  ],
  targets: [
    .target(
      name: "BridgethingTidalGlue",
      dependencies: [
        .product(name: "BridgethingGlue", package: "Bridgething"),
        .product(name: "BridgethingGateway", package: "Bridgething"),
        .product(name: "BridgethingSchema", package: "Bridgething"),
      ],
      path: "Sources/BridgethingTidalGlue"
    ),
  ]
)
