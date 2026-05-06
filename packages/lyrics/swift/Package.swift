// swift-tools-version: 6.3
import PackageDescription

let package = Package(
  name: "BridgethingLyrics",
  platforms: [
    .iOS(.v18),
    .macOS(.v15),
  ],
  products: [
    .library(name: "BridgethingLyrics", targets: ["BridgethingLyrics"]),
  ],
  targets: [
    .target(
      name: "BridgethingLyrics",
      path: "Sources/BridgethingLyrics"
    ),
    .testTarget(
      name: "BridgethingLyricsTests",
      dependencies: ["BridgethingLyrics"],
      path: "Tests/BridgethingLyricsTests"
    ),
  ]
)
