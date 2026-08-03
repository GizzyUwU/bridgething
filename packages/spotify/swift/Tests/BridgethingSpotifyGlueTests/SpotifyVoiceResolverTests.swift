import BridgethingCompanion
import BridgethingSchema
import Foundation
import Spotify
import XCTest

@testable import BridgethingSpotifyGlue

final class SpotifyVoiceResolverTests: XCTestCase {
    private enum ResolveFailure: Swift.Error { case offline }

    private func prediction(_ intent: String, _ slots: NluMutableSlots) -> NluPrediction {
        NluPrediction(intent: intent, slots: slots, transcript: "spoken")
    }

    func testTargetAndTypeMapOntoTheRequest() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(
            prediction("PLAY", NluMutableSlots(target: "  Hounds of Love ", targetType: .album))
        )
        XCTAssertEqual(fake.voiceResolveCalls.count, 1)
        XCTAssertEqual(fake.voiceResolveCalls.first?.target, "Hounds of Love", "surrounding whitespace is not part of the query")
        XCTAssertEqual(fake.voiceResolveCalls.first?.targetType, .album)
    }

    func testPodcastTargetTypeMapsToShow() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(prediction("PLAY", NluMutableSlots(target: "Reply All", targetType: .podcast)))
        XCTAssertEqual(fake.voiceResolveCalls.first?.targetType, .show)
    }

    func testStationTargetTypeSurvivesToTheRequest() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(prediction("PLAY", NluMutableSlots(target: "Kate Bush", targetType: .station)))
        XCTAssertEqual(fake.voiceResolveCalls.first?.targetType, .station)
    }

    func testMoodGenreAndEraTravelAsQueryTerms() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(
            prediction("PLAY", NluMutableSlots(genre: "indie folk", mood: "chill", era: "80s"))
        )
        let req = try XCTUnwrap(fake.voiceResolveCalls.first)
        XCTAssertNil(req.target)
        XCTAssertEqual(req.mood, "chill")
        XCTAssertEqual(req.genre, "indie folk")
        XCTAssertEqual(req.era, "80s")
    }

    func testPositionTravelsWithoutATarget() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(prediction("PLAY", NluMutableSlots(position: 3)))
        XCTAssertEqual(fake.voiceResolveCalls.first?.position, 3, "a position counts into whatever is playing")
    }

    func testRandomPopularityAloneStillResolves() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(prediction("PLAY", NluMutableSlots(popularityFilter: .random)))
        XCTAssertEqual(fake.voiceResolveCalls.first?.popularityFilter, .random, "\"play something\" is a fresh pick, not a resume")
    }

    func testResolvedUriAndContextUriLandInSlots() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        fake.voiceResolved = Spotify.VoiceResolved(
            uri: "spotify:track:7", contextUri: "spotify:album:2", display: "Cloudbusting", kind: .track, alternatives: []
        )
        let resolver = SpotifyVoiceResolver(client: fake)
        let decorated = try await resolver.decorate(prediction("PLAY", NluMutableSlots(target: "cloudbusting")))
        XCTAssertEqual(decorated.slots.uri, "spotify:track:7")
        XCTAssertEqual(decorated.slots.contextUri, "spotify:album:2")
    }

    func testResolutionWithoutAContextLeavesContextUriUnset() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        let decorated = try await resolver.decorate(prediction("PLAY", NluMutableSlots(target: "mix")))
        XCTAssertEqual(decorated.slots.uri, "spotify:playlist:1")
        XCTAssertNil(decorated.slots.contextUri)
    }

    func testCatalogIntentsAllResolve() async throws {
        for intent in ["PLAY", "ADD_TO_QUEUE", "ADD_TO_PLAYLIST", "SEARCH", "THUMBS_UP"] {
            let fake = SpotifyGlueTests.FakeClient()
            let resolver = SpotifyVoiceResolver(client: fake)
            let decorated = try await resolver.decorate(prediction(intent, NluMutableSlots(target: "mix")))
            XCTAssertEqual(decorated.slots.uri, "spotify:playlist:1", "\(intent) names catalog content")
        }
    }

    func testNonCatalogIntentPassesThroughUntouched() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        let original = prediction("SET_VOLUME", NluMutableSlots(target: "loud", level: 80))
        let decorated = try await resolver.decorate(original)
        XCTAssertTrue(fake.voiceResolveCalls.isEmpty, "a volume verb never names catalog content")
        XCTAssertEqual(decorated.slots, original.slots)
    }

    func testBarePlayResumeNeverSearches() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        let decorated = try await resolver.decorate(prediction("PLAY", NluMutableSlots()))
        XCTAssertTrue(fake.voiceResolveCalls.isEmpty, "a bare resume has nothing to resolve")
        XCTAssertNil(decorated.slots.uri)
    }

    func testTargetTypeAloneIsNotARequest() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(prediction("PLAY", NluMutableSlots(targetType: .album)))
        XCTAssertTrue(fake.voiceResolveCalls.isEmpty, "a kind narrows a request, it cannot be one")
    }

    func testBlankTargetIsNotARequest() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        let resolver = SpotifyVoiceResolver(client: fake)
        _ = try await resolver.decorate(prediction("PLAY", NluMutableSlots(target: "   ")))
        XCTAssertTrue(fake.voiceResolveCalls.isEmpty)
    }

    func testResolverFailureSurfacesToTheCaller() async throws {
        let fake = SpotifyGlueTests.FakeClient()
        fake.voiceResolveFailure = ResolveFailure.offline
        let resolver = SpotifyVoiceResolver(client: fake)
        do {
            _ = try await resolver.decorate(prediction("PLAY", NluMutableSlots(target: "mix")))
            XCTFail("expected the client failure to propagate")
        } catch {}
    }

    func testGlueExposesNoResolverBeforeAttach() {
        let glue = SpotifyGlue(
            workerBase: "https://example/auth",
            psk: "psk",
            deviceId: "dev",
            tokenStore: SpotifyGlueTests.FakeTokenStore(refresh: "rt"),
            clientFactory: { _, _ in SpotifyGlueTests.FakeClient() }
        )
        XCTAssertNil(glue.voiceResolver(), "an unattached glue holds no client to resolve through")
    }
}
