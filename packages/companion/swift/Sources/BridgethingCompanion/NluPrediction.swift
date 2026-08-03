import BridgethingSchema
import Foundation

public struct NluPrediction: Sendable {
    public var intent: String
    public var slots: NluMutableSlots
    public var transcript: String
    public var alternates: [NluAlternate]?

    public init(
        intent: String,
        slots: NluMutableSlots = .init(),
        transcript: String,
        alternates: [NluAlternate]? = nil
    ) {
        self.intent = intent
        self.slots = slots
        self.transcript = transcript
        self.alternates = alternates
    }

    public func toWire() -> NluResolvedIntent {
        NluResolvedIntent(
            intent: intent,
            slots: slots.toWire(),
            transcript: transcript,
            alternates: alternates
        )
    }

    public static func fromWire(_ r: NluResolvedIntent) -> NluPrediction {
        NluPrediction(
            intent: r.intent,
            slots: NluMutableSlots.fromWire(r.slots),
            transcript: r.transcript,
            alternates: r.alternates
        )
    }
}

public struct NluMutableSlots: Equatable, Sendable {
    public var target: String?
    public var targetType: NluTargetType?
    public var playlist: String?
    public var genre: String?
    public var mood: String?
    public var era: String?
    public var popularityFilter: NluPopularityFilter?
    public var position: UInt32?
    public var count: UInt32?
    public var scope: NluScope?
    public var enabled: Bool?
    public var mute: Bool?
    public var repeatMode: NluRepeatMode?
    public var seconds: Int32?
    public var speed: NluPlaybackSpeed?
    public var direction: NluDirection?
    public var amount: NluAmount?
    public var level: UInt32?
    public var preset: String?
    public var view: NluView?
    public var phoneAction: NluPhoneAction?
    public var webappName: String?
    public var uri: String?
    public var contextUri: String?

    public init(
        target: String? = nil,
        targetType: NluTargetType? = nil,
        playlist: String? = nil,
        genre: String? = nil,
        mood: String? = nil,
        era: String? = nil,
        popularityFilter: NluPopularityFilter? = nil,
        position: UInt32? = nil,
        count: UInt32? = nil,
        scope: NluScope? = nil,
        enabled: Bool? = nil,
        mute: Bool? = nil,
        repeatMode: NluRepeatMode? = nil,
        seconds: Int32? = nil,
        speed: NluPlaybackSpeed? = nil,
        direction: NluDirection? = nil,
        amount: NluAmount? = nil,
        level: UInt32? = nil,
        preset: String? = nil,
        view: NluView? = nil,
        phoneAction: NluPhoneAction? = nil,
        webappName: String? = nil,
        uri: String? = nil,
        contextUri: String? = nil
    ) {
        self.target = target
        self.targetType = targetType
        self.playlist = playlist
        self.genre = genre
        self.mood = mood
        self.era = era
        self.popularityFilter = popularityFilter
        self.position = position
        self.count = count
        self.scope = scope
        self.enabled = enabled
        self.mute = mute
        self.repeatMode = repeatMode
        self.seconds = seconds
        self.speed = speed
        self.direction = direction
        self.amount = amount
        self.level = level
        self.preset = preset
        self.view = view
        self.phoneAction = phoneAction
        self.webappName = webappName
        self.uri = uri
        self.contextUri = contextUri
    }

    public func toWire() -> NluSlots {
        NluSlots(
            target: target,
            targetType: targetType,
            playlist: playlist,
            genre: genre,
            mood: mood,
            era: era,
            popularityFilter: popularityFilter,
            position: position,
            count: count,
            scope: scope,
            enabled: enabled,
            mute: mute,
            repeatMode: repeatMode,
            seconds: seconds,
            speed: speed,
            direction: direction,
            amount: amount,
            level: level,
            preset: preset,
            view: view,
            phoneAction: phoneAction,
            webappName: webappName,
            uri: uri,
            contextUri: contextUri
        )
    }

    public static func fromWire(_ s: NluSlots) -> NluMutableSlots {
        NluMutableSlots(
            target: s.target,
            targetType: s.targetType,
            playlist: s.playlist,
            genre: s.genre,
            mood: s.mood,
            era: s.era,
            popularityFilter: s.popularityFilter,
            position: s.position,
            count: s.count,
            scope: s.scope,
            enabled: s.enabled,
            mute: s.mute,
            repeatMode: s.repeatMode,
            seconds: s.seconds,
            speed: s.speed,
            direction: s.direction,
            amount: s.amount,
            level: s.level,
            preset: s.preset,
            view: s.view,
            phoneAction: s.phoneAction,
            webappName: s.webappName,
            uri: s.uri,
            contextUri: s.contextUri
        )
    }
}
