import BridgethingGlue
import Foundation
import XCTest

@testable import BridgethingAppleMusicGlue

final class MusicKitMappingTests: XCTestCase {
    func testLibraryIdClassification() {
        XCTAssertTrue(isLibraryId("i.addWKtl6kQE"))
        XCTAssertTrue(isLibraryId("p.PkdJvPXIVoJV1zl"))
        XCTAssertTrue(isLibraryId("l.qrJJhpu"))
        XCTAssertTrue(isLibraryId("r.dMSRoKN"))

        XCTAssertFalse(isLibraryId("1440857781"))
        XCTAssertFalse(isLibraryId("pl.u-oZylDpJTLqmvvXo"))
        XCTAssertFalse(isLibraryId("pl.f4d106fed2bd41149aaacabb233eb5eb"))
        XCTAssertFalse(isLibraryId("ra.978194965"))
        XCTAssertFalse(isLibraryId(""))
        XCTAssertFalse(isLibraryId("i."))
    }

    func testSizedArtworkUrlHandlesCatalogAndLibraryTemplates() {
        XCTAssertEqual(
            sizedArtworkUrl("https://is1-ssl.mzstatic.com/image/thumb/a/{w}x{h}bb.jpg", edge: 248),
            "https://is1-ssl.mzstatic.com/image/thumb/a/248x248bb.jpg"
        )
        XCTAssertEqual(
            sizedArtworkUrl("musicKit://artwork/library/5F37858D-F46B/{w}x{h}?at=item&mt=music", edge: 96),
            "musicKit://artwork/library/5F37858D-F46B/96x96?at=item&mt=music"
        )
    }

    func testImageCodecRoundTripsMusicKitUrl() {
        let codec = ImageAssetCodec(namespace: "applemusic/img/")
        let url = "musicKit://artwork/library/5F37858D-F46B/248x248?at=item&fat=&id=771867&mt=music&aat=Music122/v4/37/25/f5/x.jpg"
        let id = codec.assetId(url: url, maxEdge: 248)
        XCTAssertNotNil(id)
        let parsed = codec.parse(id!)
        XCTAssertEqual(parsed?.url.absoluteString, url)
        XCTAssertEqual(parsed?.maxEdge, 248)
    }

    func testArtSessionRoutesByScheme() {
        let glue = AppleMusicGlue()
        XCTAssertTrue(glue.artSession(for: URL(string: "musicKit://artwork/library/x/248x248")!) === URLSession.shared)
        XCTAssertFalse(glue.artSession(for: URL(string: "https://is1-ssl.mzstatic.com/x.jpg")!) === URLSession.shared)
    }
}
