import BridgethingSchema
import Foundation

public enum NluIntentValidator {
    public struct SnapTarget: Equatable, Sendable {
        public let intent: String
        public let slots: NluMutableSlots?

        public init(_ intent: String, _ slots: NluMutableSlots? = nil) {
            self.intent = intent
            self.slots = slots
        }
    }

    public static let synonymMap: [String: SnapTarget] = [
        "BACK_15": .init("SEEK_RELATIVE", .init(seconds: -15)),
        "BACK_15_SECONDS": .init("SEEK_RELATIVE", .init(seconds: -15)),
        "CONTINUE": .init("PLAY"),
        "CURRENT_SONG": .init("WHATS_PLAYING"),
        "FAST_FORWARD": .init("SEEK_RELATIVE", .init(seconds: 15)),
        "FOLLOW_ARTIST": .init("ADD_TO_COLLECTION"),
        "FORWARD_15": .init("SEEK_RELATIVE", .init(seconds: 15)),
        "FORWARD_15_SECONDS": .init("SEEK_RELATIVE", .init(seconds: 15)),
        "GO_BACK": .init("PREVIOUS"),
        "GO_BACK_15_SECONDS": .init("SEEK_RELATIVE", .init(seconds: -15)),
        "GO_FORWARD_15_SECONDS": .init("SEEK_RELATIVE", .init(seconds: 15)),
        "LAUNCH_APP": .init("OPEN_WEBAPP"),
        "LAUNCH_WEBAPP": .init("OPEN_WEBAPP"),
        "LIKE": .init("THUMBS_UP"),
        "LOOP": .init("SET_REPEAT", .init(repeatMode: .all)),
        "LOOP_ONE": .init("SET_REPEAT", .init(repeatMode: .one)),
        "LOVE": .init("THUMBS_UP"),
        "MORE_BY_ARTIST": .init("SHOW_VIEW", .init(view: .thisArtist)),
        "MORE_BY_THIS_ARTIST": .init("SHOW_VIEW", .init(view: .thisArtist)),
        "MORE_FROM_THIS_ARTIST": .init("SHOW_VIEW", .init(view: .thisArtist)),
        "NEXT_EPISODE": .init("NEXT"),
        "NORMAL_SPEED": .init("SET_PLAYBACK_SPEED", .init(speed: .one)),
        "OPEN_APP": .init("OPEN_WEBAPP"),
        "OPEN_MY_LIBRARY": .init("SHOW_VIEW", .init(view: .library)),
        "OPEN_SETTINGS": .init("NO_INTENT"),
        "PLAYBACK_SPEED_FASTER": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointFive)),
        "PLAYBACK_SPEED_NORMAL": .init("SET_PLAYBACK_SPEED", .init(speed: .one)),
        "PLAY_MY_LIBRARY": .init("PLAY"),
        "PLAY_MY_SONGS": .init("PLAY"),
        "PLAY_MY_TOP_50": .init("PLAY"),
        "PLAY_NEW_EPISODE": .init("PLAY"),
        "PLAY_NEXT": .init("NEXT"),
        "PLAY_NEXT_EPISODE": .init("NEXT"),
        "PLAY_PLAYLIST": .init("PLAY"),
        "PLAY_PREVIOUS": .init("PREVIOUS"),
        "PLAY_RADIO": .init("PLAY"),
        "PLAY_RECENT": .init("PLAY"),
        "PLAY_RECENT_PLAYLIST": .init("PLAY"),
        "PLAY_THE_NEXT_EPISODE": .init("NEXT"),
        "PLAY_THE_PREVIOUS_EPISODE": .init("PREVIOUS"),
        "PLAY_THIS_PROGRAM": .init("PLAY"),
        "PLAY_THIS_SONG": .init("PLAY"),
        "PREVIOUS_EPISODE": .init("PREVIOUS"),
        "RANDOM": .init("SET_SHUFFLE", .init(enabled: true)),
        "RANDOMIZE": .init("SET_SHUFFLE", .init(enabled: true)),
        "REPEAT": .init("SET_REPEAT", .init(repeatMode: .all)),
        "REPEAT_THIS": .init("SET_REPEAT", .init(repeatMode: .one)),
        "REPEAT_TRACK": .init("SET_REPEAT", .init(repeatMode: .one)),
        "RESTART": .init("PLAY"),
        "REWIND": .init("SEEK_RELATIVE", .init(seconds: -15)),
        "REWIND_15_SECONDS": .init("SEEK_RELATIVE", .init(seconds: -15)),
        "SAVE": .init("ADD_TO_COLLECTION"),
        "SAVE_TO_COLLECTION": .init("ADD_TO_COLLECTION"),
        "SAVE_TO_LIBRARY": .init("ADD_TO_COLLECTION"),
        "SET_ABSOLUTE_PLAYBACK_SPEED": .init("SET_PLAYBACK_SPEED", .init(speed: .one)),
        "SET_ABSOLUTE_SPEED": .init("SET_PLAYBACK_SPEED", .init(speed: .one)),
        "SET_PLAYBACK_SPEED_1POINT_2X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointTwo)),
        "SET_PLAYBACK_SPEED_1POINT_5X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointFive)),
        "SET_PLAYBACK_SPEED_1_2X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointTwo)),
        "SET_PLAYBACK_SPEED_1_5X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointFive)),
        "SET_PLAYBACK_SPEED_1_POINT_2X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointTwo)),
        "SET_PLAYBACK_SPEED_1_POINT_5X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointFive)),
        "SET_PLAYBACK_SPEED_FASTER": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointFive)),
        "SET_SPEED": .init("SET_PLAYBACK_SPEED", .init(speed: .one)),
        "SET_SPEED_1_2X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointTwo)),
        "SET_SPEED_1_5X": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointFive)),
        "SET_SPEED_2X": .init("SET_PLAYBACK_SPEED", .init(speed: .two)),
        "SHOW_LIBRARY": .init("SHOW_VIEW", .init(view: .library)),
        "SHOW_LYRICS": .init("WHATS_PLAYING"),
        "SHOW_MORE_BY_ARTIST": .init("SHOW_VIEW", .init(view: .thisArtist)),
        "SHOW_MORE_LIKE_THIS": .init("MORE_LIKE_THIS"),
        "SHOW_MY_NEW_PODCAST_EPISODES": .init("SHOW_VIEW", .init(view: .newEpisodes)),
        "SHOW_MY_PLAYLIST": .init("SHOW_VIEW", .init(view: .library)),
        "SHOW_MY_PLAYLISTS": .init("SHOW_VIEW", .init(view: .library)),
        "SHOW_MY_PODCAST_EPISODES": .init("SHOW_VIEW", .init(view: .savedEpisodes)),
        "SHOW_MY_QUEUE": .init("SHOW_VIEW", .init(view: .queue)),
        "SHOW_MY_RECENT_PLAYLISTS": .init("SHOW_VIEW", .init(view: .library)),
        "SHOW_NEW_EPISODES": .init("SHOW_VIEW", .init(view: .newEpisodes)),
        "SHOW_NEXT": .init("NEXT"),
        "SHOW_PLAYING": .init("WHATS_PLAYING"),
        "SHOW_PLAYLIST": .init("SHOW_VIEW", .init(view: .library)),
        "SHOW_PLAYLISTS": .init("SHOW_VIEW", .init(view: .library)),
        "SHOW_PRESETS": .init("SHOW_VIEW", .init(view: .presets)),
        "SHOW_QUEUE": .init("SHOW_VIEW", .init(view: .queue)),
        "SHOW_SAVED_EPISODES": .init("SHOW_VIEW", .init(view: .savedEpisodes)),
        "SHOW_SONGS": .init("SHOW_VIEW", .init(view: .songs)),
        "SHOW_THE_LIBRARY": .init("SHOW_VIEW", .init(view: .library)),
        "SHOW_THE_NEW_EPISODES": .init("SHOW_VIEW", .init(view: .newEpisodes)),
        "SHOW_THE_PREVIOUS_PLAYLIST": .init("PREVIOUS"),
        "SHOW_THE_SONGS": .init("SHOW_VIEW", .init(view: .songs)),
        "SHOW_THIS_SONG": .init("WHATS_PLAYING"),
        "SHOW_THIS_TRACK": .init("WHATS_PLAYING"),
        "SHUFFLE": .init("SET_SHUFFLE", .init(enabled: true)),
        "SHUT_OFF": .init("PAUSE"),
        "SILENCE": .init("SET_MUTE", .init(enabled: true)),
        "SKIP_BACK_15_SECONDS": .init("SEEK_RELATIVE", .init(seconds: -15)),
        "SKIP_FORWARD_15_SECONDS": .init("SEEK_RELATIVE", .init(seconds: 15)),
        "SKIP_TO_NEXT": .init("NEXT"),
        "SLOW_DOWN": .init("SET_PLAYBACK_SPEED", .init(speed: .one)),
        "SOUND_ON": .init("SET_MUTE", .init(enabled: false)),
        "SPEED_UP": .init("SET_PLAYBACK_SPEED", .init(speed: .onePointFive)),
        "SWITCH_APP": .init("OPEN_WEBAPP"),
        "SWITCH_TO_WEBAPP": .init("OPEN_WEBAPP"),
        "TELL_A_JOKE": .init("NO_INTENT"),
        "TELL_ME_A_JOKE": .init("NO_INTENT"),
        "TURN_OFF": .init("PAUSE"),
        "UNPAUSE": .init("PLAY"),
        "WHATS_PLAYING_NOW": .init("WHATS_PLAYING"),
        "WHATS_THIS": .init("WHATS_PLAYING"),
        "WHATS_THIS_SONG": .init("WHATS_PLAYING"),
        "WHATS_THIS_TRACK": .init("WHATS_PLAYING"),
    ]

    public static let validIntents: Set<String> = [
        "CLARIFY",
        "NO_INTENT",
        "ADD_TO_COLLECTION",
        "ADD_TO_PLAYLIST",
        "ADD_TO_QUEUE",
        "CANCEL_INTERACTION",
        "CLARIFY",
        "HELP",
        "MORE_LIKE_THIS",
        "NEXT",
        "NO_INTENT",
        "OPEN_WEBAPP",
        "PAUSE",
        "PHONE_ACTION",
        "PLAY",
        "PLAY_PRESET",
        "PREVIOUS",
        "SAVE_TO_PRESET",
        "SEARCH",
        "SEEK_RELATIVE",
        "SET_BRIGHTNESS",
        "SET_DISCOVERABLE",
        "SET_MUTE",
        "SET_PLAYBACK_SPEED",
        "SET_REPEAT",
        "SET_SHUFFLE",
        "SET_VOLUME",
        "SHOW",
        "SHOW_VIEW",
        "SYSTEM_ACTION",
        "THUMBS_UP",
        "WHATS_PLAYING",
    ]

    public enum SnapReason: Equatable, Sendable {
        case exact
        case synonym
        case edit(Int)
        case noMatch
    }

    public static func snapIntent(_ raw: String?, editMax: Int = 3) -> (SnapTarget?, SnapReason) {
        guard let raw, !raw.isEmpty else { return (nil, .noMatch) }
        if validIntents.contains(raw) { return (.init(raw), .exact) }
        let rawU = raw.uppercased().trimmingCharacters(in: .whitespaces)
        if let mapped = synonymMap[rawU] { return (mapped, .synonym) }
        if validIntents.contains(rawU) { return (.init(rawU), .exact) }

        var best: (Int, String?) = (editMax + 1, nil)
        for valid in validIntents {
            let d = levenshtein(rawU, valid)
            if d < best.0 { best = (d, valid) }
        }
        if let target = best.1, best.0 <= editMax {
            return (.init(target), .edit(best.0))
        }
        return (nil, .noMatch)
    }

    public static func snapPrediction(_ pred: NluPrediction?) -> (NluPrediction?, SnapReason) {
        guard let pred else { return (nil, .noMatch) }
        let (snapped, reason) = snapIntent(pred.intent)
        guard let target = snapped else { return (nil, reason) }

        var next = pred
        next.intent = target.intent
        if let snapped = target.slots {
            if next.slots.enabled == nil { next.slots.enabled = snapped.enabled }
            if next.slots.repeatMode == nil { next.slots.repeatMode = snapped.repeatMode }
            if next.slots.seconds == nil { next.slots.seconds = snapped.seconds }
            if next.slots.speed == nil { next.slots.speed = snapped.speed }
            if next.slots.direction == nil { next.slots.direction = snapped.direction }
            if next.slots.view == nil { next.slots.view = snapped.view }
        }
        return (next, reason)
    }

    static func levenshtein(_ a: String, _ b: String) -> Int {
        if a == b { return 0 }
        let aChars = Array(a)
        let bChars = Array(b)
        if aChars.isEmpty { return bChars.count }
        if bChars.isEmpty { return aChars.count }

        var prev = Array(0...bChars.count)
        var cur = Array(repeating: 0, count: bChars.count + 1)
        for i in 1...aChars.count {
            cur[0] = i
            for j in 1...bChars.count {
                let cost = aChars[i - 1] == bChars[j - 1] ? 0 : 1
                cur[j] = min(cur[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost)
            }
            swap(&prev, &cur)
        }
        return prev[bChars.count]
    }
}
