// swift-tools-version: 6.3
import PackageDescription

let package = Package(
  name: "Bridgething",
  platforms: [
    .iOS(.v16),
    .macOS(.v13),
  ],
  products: [
    .library(name: "BridgethingSchema", targets: ["BridgethingSchema"]),
    .library(name: "BridgethingGateway", targets: ["BridgethingGateway"]),
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
  ]
)
