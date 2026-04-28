// swift-tools-version:5.9
import PackageDescription

let package = Package(
  name: "Bridgething",
  platforms: [
    .iOS(.v15),
    .macOS(.v12),
  ],
  products: [
    .library(name: "BridgethingSchema", targets: ["BridgethingSchema"]),
  ],
  targets: [
    .target(
      name: "BridgethingSchema",
      path: "lib/swift/Sources/BridgethingSchema"
    )
  ]
)
