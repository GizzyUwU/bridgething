import Foundation
import XCTest
@testable import BridgethingCompanion

final class OtaManifestTests: XCTestCase {
  func testCompositeVersionParse() {
    let v = OtaCompositeVersion.parse("0.8.4+image.2026.05.0")
    XCTAssertEqual(v?.daemon, "0.8.4")
    XCTAssertEqual(v?.image, "2026.05.0")

    XCTAssertNil(OtaCompositeVersion.parse("0.8.4"))
    XCTAssertNil(OtaCompositeVersion.parse("0.8.4+2026.05.0"))
    XCTAssertNil(OtaCompositeVersion.parse("+image.2026.05.0"))
    XCTAssertNil(OtaCompositeVersion.parse("0.8.4+image."))
  }

  func testArtifactURLs() {
    let urls = OtaArtifactURLs(
      rootURL: URL(string: "https://ota.bridgething.com")!,
      channel: "stable",
      daemonVersion: "0.8.4",
      imageVersion: "2026.05.0",
      imageVariant: "prod"
    )
    XCTAssertEqual(urls.daemonBinary.absoluteString, "https://ota.bridgething.com/daemon/stable/0.8.4/bridgething")
    XCTAssertEqual(urls.imageSwu.absoluteString, "https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image.swu")
    XCTAssertEqual(urls.imageZck.absoluteString, "https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image.zck")
  }

  func testManifestDecode() throws {
    let json = """
    {
      "manifest_version": 1,
      "updated_at": "2026-05-30T00:00:00Z",
      "channels": {
        "stable": {
          "name": "stable", "stability": "stable", "default": true,
          "latest": "0.8.4+image.2026.05.0",
          "releases": ["0.8.4+image.2026.05.0", "0.8.3+image.2026.04.0"]
        }
      },
      "releases": {
        "0.8.4+image.2026.05.0": {"version": "0.8.4+image.2026.05.0", "channel": "stable", "deprecated": false},
        "0.8.3+image.2026.04.0": {"version": "0.8.3+image.2026.04.0", "channel": "stable", "yanked": "bad build", "deprecated": false}
      }
    }
    """
    let manifest = try JSONDecoder().decode(OtaDiscoverManifest.self, from: Data(json.utf8))
    XCTAssertEqual(manifest.channels["stable"]?.latest, "0.8.4+image.2026.05.0")
    XCTAssertEqual(manifest.channels["stable"]?.releases.count, 2)
    XCTAssertNil(manifest.releases["0.8.4+image.2026.05.0"]?.yanked)
    XCTAssertEqual(manifest.releases["0.8.3+image.2026.04.0"]?.yanked, "bad build")
  }
}
