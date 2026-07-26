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
    public var webappName: String?
    public var preset: String?
    public var enabled: Bool?
    public var repeatMode: NluRepeatMode?
    public var seconds: Int32?
    public var speed: NluPlaybackSpeed?
    public var direction: NluDirection?
    public var amount: String?
    public var level: UInt32?
    public var brightnessMode: NluBrightnessMode?
    public var view: NluView?
    public var phoneAction: NluPhoneAction?
    public var systemAction: NluSystemAction?
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
        webappName: String? = nil,
        preset: String? = nil,
        enabled: Bool? = nil,
        repeatMode: NluRepeatMode? = nil,
        seconds: Int32? = nil,
        speed: NluPlaybackSpeed? = nil,
        direction: NluDirection? = nil,
        amount: String? = nil,
        level: UInt32? = nil,
        brightnessMode: NluBrightnessMode? = nil,
        view: NluView? = nil,
        phoneAction: NluPhoneAction? = nil,
        systemAction: NluSystemAction? = nil,
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
        self.webappName = webappName
        self.preset = preset
        self.enabled = enabled
        self.repeatMode = repeatMode
        self.seconds = seconds
        self.speed = speed
        self.direction = direction
        self.amount = amount
        self.level = level
        self.brightnessMode = brightnessMode
        self.view = view
        self.phoneAction = phoneAction
        self.systemAction = systemAction
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
            webappName: webappName,
            preset: preset,
            enabled: enabled,
            repeatMode: repeatMode,
            seconds: seconds,
            speed: speed,
            direction: direction,
            amount: amount,
            level: level,
            brightnessMode: brightnessMode,
            view: view,
            phoneAction: phoneAction,
            systemAction: systemAction,
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
            webappName: s.webappName,
            preset: s.preset,
            enabled: s.enabled,
            repeatMode: s.repeatMode,
            seconds: s.seconds,
            speed: s.speed,
            direction: s.direction,
            amount: s.amount,
            level: s.level,
            brightnessMode: s.brightnessMode,
            view: s.view,
            phoneAction: s.phoneAction,
            systemAction: s.systemAction,
            uri: s.uri
        )
    }
}
