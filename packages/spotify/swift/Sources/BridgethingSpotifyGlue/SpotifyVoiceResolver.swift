import BridgethingCompanion
import BridgethingSchema
import Foundation
import Spotify

public final class SpotifyVoiceResolver: VoiceCatalogResolving {
    private let client: any SpotifyClientProviding

    init(client: any SpotifyClientProviding) {
        self.client = client
    }

    public func decorate(_ prediction: NluPrediction) async throws -> NluPrediction {
        guard Self.isCatalogIntent(prediction.intent), let req = Self.request(from: prediction.slots) else {
            return prediction
        }
        let resolved = try await client.resolveVoice(req: req)
        var decorated = prediction
        decorated.slots.uri = resolved.uri
        decorated.slots.contextUri = resolved.contextUri
        return decorated
    }

    private static func isCatalogIntent(_ intent: String) -> Bool {
        switch intent {
        case "PLAY", "ADD_TO_QUEUE", "ADD_TO_PLAYLIST", "SEARCH", "THUMBS_UP": true
        default: false
        }
    }

    private static func request(from slots: NluMutableSlots) -> Spotify.VoiceResolveRequest? {
        let target = text(slots.target)
        let mood = text(slots.mood)
        let genre = text(slots.genre)
        let era = text(slots.era)
        let popularity = popularity(slots.popularityFilter)
        guard target != nil || mood != nil || genre != nil || era != nil
            || slots.position != nil || popularity != nil
        else { return nil }
        return Spotify.VoiceResolveRequest(
            target: target,
            targetType: kind(slots.targetType),
            mood: mood,
            genre: genre,
            era: era,
            popularityFilter: popularity,
            position: slots.position
        )
    }

    private static func text(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespaces), !trimmed.isEmpty else { return nil }
        return trimmed
    }

    private static func kind(_ type: NluTargetType?) -> Spotify.VoiceTargetKind? {
        switch type {
        case .artist: .artist
        case .track: .track
        case .album: .album
        case .playlist: .playlist
        case .podcast: .show
        case .episode: .episode
        case .station: .station
        case nil: nil
        }
    }

    private static func popularity(_ filter: NluPopularityFilter?) -> Spotify.VoicePopularity? {
        switch filter {
        case .top5: .top5
        case .top10: .top10
        case .popular: .popular
        case .recent: .recent
        case .new: .new
        case .random: .random
        case nil: nil
        }
    }
}
