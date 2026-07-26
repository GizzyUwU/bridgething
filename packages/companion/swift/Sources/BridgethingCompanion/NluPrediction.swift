import BridgethingSchema
import Foundation

public struct NluPrediction: Sendable {
    public var intent: String
    public var slots: NluMutableSlots
    public var transcript: String
    public var confidence: NluConfidence?
    public var alternates: [NluAlternate]?

    public init(
        intent: String,
        slots: NluMutableSlots = .init(),
        transcript: String,
        confidence: NluConfidence? = nil,
        alternates: [NluAlternate]? = nil
    ) {
        self.intent = intent
        self.slots = slots
        self.transcript = transcript
        self.confidence = confidence
        self.alternates = alternates
    }

    public func toWire() -> NluResolvedIntent {
        NluResolvedIntent(
            intent: intent,
            slots: slots.toWire(),
            transcript: transcript,
            confidence: confidence,
            alternates: alternates
        )
    }

    public static func fromWire(_ r: NluResolvedIntent) -> NluPrediction {
        NluPrediction(
            intent: r.intent,
            slots: r.slots.map(NluMutableSlots.fromWire) ?? NluMutableSlots(),
            transcript: r.transcript,
            confidence: r.confidence,
            alternates: r.alternates
        )
    }
}

public struct NluMutableSlots: Equatable, Sendable {
    public var artist: String?
    public var track: String?
    public var album: String?
    public var playlist: String?
    public var podcast: String?
    public var episode: String?
    public var mood: String?
    public var genre: String?
    public var era: String?
    public var popularityFilter: String?
    public var entityType: String?
    public var query: String?
    public var rawQuery: String?
    public var webappId: String?
    public var webappName: String?
    public var preset: String?
    public var amount: String?
    public var level: UInt32?
    public var uri: String?

    public init(
        artist: String? = nil,
        track: String? = nil,
        album: String? = nil,
        playlist: String? = nil,
        podcast: String? = nil,
        episode: String? = nil,
        mood: String? = nil,
        genre: String? = nil,
        era: String? = nil,
        popularityFilter: String? = nil,
        entityType: String? = nil,
        query: String? = nil,
        rawQuery: String? = nil,
        webappId: String? = nil,
        webappName: String? = nil,
        preset: String? = nil,
        amount: String? = nil,
        level: UInt32? = nil,
        uri: String? = nil
    ) {
        self.artist = artist
        self.track = track
        self.album = album
        self.playlist = playlist
        self.podcast = podcast
        self.episode = episode
        self.mood = mood
        self.genre = genre
        self.era = era
        self.popularityFilter = popularityFilter
        self.entityType = entityType
        self.query = query
        self.rawQuery = rawQuery
        self.webappId = webappId
        self.webappName = webappName
        self.preset = preset
        self.amount = amount
        self.level = level
        self.uri = uri
    }

    public func toWire() -> NluSlots {
        NluSlots(
            artist: artist,
            track: track,
            album: album,
            playlist: playlist,
            podcast: podcast,
            episode: episode,
            mood: mood,
            genre: genre,
            era: era,
            popularityFilter: popularityFilter,
            entityType: entityType,
            query: query,
            rawQuery: rawQuery,
            webappId: webappId,
            webappName: webappName,
            preset: preset,
            amount: amount,
            level: level,
            uri: uri
        )
    }

    public static func fromWire(_ s: NluSlots) -> NluMutableSlots {
        NluMutableSlots(
            artist: s.artist,
            track: s.track,
            album: s.album,
            playlist: s.playlist,
            podcast: s.podcast,
            episode: s.episode,
            mood: s.mood,
            genre: s.genre,
            era: s.era,
            popularityFilter: s.popularityFilter,
            entityType: s.entityType,
            query: s.query,
            rawQuery: s.rawQuery,
            webappId: s.webappId,
            webappName: s.webappName,
            preset: s.preset,
            amount: s.amount,
            level: s.level,
            uri: s.uri
        )
    }
}
