import BridgethingSchema
import Foundation

/// Snap hallucinated intent strings to the nearest valid intent. Two-pass:
/// synonym map first (well-known close-misses mined from SFT failure tails),
/// then a Levenshtein-3 fallback. Also strips fillers from WEBAPP_INTENT
/// raw_query slots so the wire matches what a webapp grammar handler expects.
///
/// Mirrors `nlu/scripts/intent_validator.py::snap_prediction` - keep the
/// synonym map in sync with the Python source when retraining surfaces new
/// hallucination patterns.
public enum NluIntentValidator {
    public static let synonymMap: [String: String] = [
        "SET_PLAYBACK_SPEED_1_2X": "SET_PLAYBACK_SPEED_1POINT2X",
        "SET_PLAYBACK_SPEED_1_5X": "SET_PLAYBACK_SPEED_1POINT5X",
        "SET_PLAYBACK_SPEED": "SET_PLAYBACK_SPEED_1X",
        "SET_PLAYBACK_SPEED_FASTER": "SET_PLAYBACK_SPEED_1POINT5X",
        "SET_ABSOLUTE_SPEED": "SET_PLAYBACK_SPEED_1X",
        "SPEED_UP": "SET_PLAYBACK_SPEED_1POINT5X",
        "SLOW_DOWN": "SET_PLAYBACK_SPEED_1X",
        "SHOW_NEW_EPISODES": "SHOW_MY_NEW_EPISODES",
        "SHOW_SAVED_EPISODES": "SHOW_MY_SAVED_EPISODES",
        "SHOW_LIBRARY": "SHOW_MY_LIBRARY",
        "SHOW_SONGS": "SHOW_MY_SONGS",
        "SHOW_PRESETS": "SHOW_MY_PRESETS",
        "SHOW_QUEUE": "SHOW_THE_QUEUE",
        "SHOW_MY_QUEUE": "SHOW_THE_QUEUE",
        "SHOW_PLAYING": "WHATS_PLAYING",
        "SHOW_THIS_TRACK": "WHATS_PLAYING",
        "SHOW_THIS_SONG": "WHATS_PLAYING",
        "WHATS_THIS_SONG": "WHATS_PLAYING",
        "WHATS_THIS_TRACK": "WHATS_PLAYING",
        "WHATS_THIS": "WHATS_PLAYING",
        "WHATS_PLAYING_NOW": "WHATS_PLAYING",
        "CURRENT_SONG": "WHATS_PLAYING",
        "PLAY_NEXT": "NEXT",
        "PLAY_PREVIOUS": "PREVIOUS",
        "GO_BACK": "PREVIOUS",
        "PLAY_RECENT": "PLAY",
        "UNPAUSE": "RESUME",
        "RESTART": "RESUME",
        "CONTINUE": "RESUME",
        "OPEN_APP": "OPEN_WEBAPP",
        "LAUNCH_APP": "OPEN_WEBAPP",
        "LAUNCH_WEBAPP": "OPEN_WEBAPP",
        "SWITCH_TO_WEBAPP": "OPEN_WEBAPP",
        "SWITCH_APP": "OPEN_WEBAPP",
        "REPEAT": "REPEAT_ON",
        "LOOP": "REPEAT_ON",
        "LOOP_ONE": "REPEAT_ONE",
        "REPEAT_TRACK": "REPEAT_ONE",
        "REPEAT_THIS": "REPEAT_ONE",
        "SILENCE": "MUTE",
        "SOUND_ON": "UNMUTE",
        "SHUFFLE": "SHUFFLE_ON",
        "RANDOM": "SHUFFLE_ON",
        "RANDOMIZE": "SHUFFLE_ON",
        "FOLLOW_ARTIST": "FOLLOW",
        "SAVE_TO_LIBRARY": "ADD_TO_COLLECTION",
        "SAVE_TO_COLLECTION": "ADD_TO_COLLECTION",
        "SAVE": "ADD_TO_COLLECTION",
        "LIKE": "THUMBS_UP",
        "LOVE": "THUMBS_UP",
        "DISLIKE": "BAN_TRACK",
        "BAN": "BAN_TRACK",
        "SKIP_AND_DONT_PLAY_AGAIN": "BAN_TRACK",
        "PLAY_MY_LIBRARY": "PLAY",
        "PLAY_MY_TOP_50": "PLAY",
        "PLAY_MY_SONGS": "PLAY",
        "PLAY_RECENT_PLAYLIST": "PLAY",
        "PLAY_PLAYLIST": "PLAY",
        "PLAY_NEW_EPISODE": "PLAY",
        "PLAY_THIS_SONG": "RESUME",
        "PLAY_THIS_PROGRAM": "RESUME",
        "PLAY_NEXT_EPISODE": "NEXT",
        "NEXT_EPISODE": "NEXT",
        "PREVIOUS_EPISODE": "PREVIOUS",
        "PLAY_THE_NEXT_EPISODE": "NEXT",
        "PLAY_THE_PREVIOUS_EPISODE": "PREVIOUS",
        "PLAY_RADIO": "PLAY",
        "SHOW_LYRICS": "WHATS_PLAYING",
        "SHOW_NEXT": "NEXT",
        "SHOW_THE_PREVIOUS_PLAYLIST": "PREVIOUS",
        "SET_OUTPUT_DEVICE": "TRANSFER_PLAYBACK",
        "MORE_FROM_THIS_ARTIST": "SHOW_THIS_ARTIST",
        "SHOW_MORE_LIKE_THIS": "MORE_LIKE_THIS",
        "SHOW_MORE_BY_ARTIST": "SHOW_THIS_ARTIST",
        "MORE_BY_ARTIST": "SHOW_THIS_ARTIST",
        "MORE_BY_THIS_ARTIST": "SHOW_THIS_ARTIST",
        "SET_ABSOLUTE_PLAYBACK_SPEED": "SET_PLAYBACK_SPEED_1X",
        "SHOW_MY_NEW_PODCAST_EPISODES": "SHOW_MY_NEW_EPISODES",
        "SHOW_MY_PODCAST_EPISODES": "SHOW_MY_SAVED_EPISODES",
        "SHOW_THE_NEW_EPISODES": "SHOW_MY_NEW_EPISODES",
        "SHOW_THE_LIBRARY": "SHOW_MY_LIBRARY",
        "SHOW_THE_SONGS": "SHOW_MY_SONGS",
        "SHOW_MY_PLAYLIST": "SHOW_MY_LIBRARY",
        "SHOW_MY_PLAYLISTS": "SHOW_MY_LIBRARY",
        "SHOW_MY_RECENT_PLAYLISTS": "SHOW_MY_LIBRARY",
        "SHOW_PLAYLISTS": "SHOW_MY_LIBRARY",
        "SHOW_PLAYLIST": "SHOW_MY_LIBRARY",
        "OPEN_MY_LIBRARY": "SHOW_MY_LIBRARY",
        "SKIP_TO_NEXT": "NEXT",
        "FORWARD_15_SECONDS": "SEEK_FORWARD_15_SECONDS",
        "REWIND_15_SECONDS": "SEEK_BACK_15_SECONDS",
        "BACK_15_SECONDS": "SEEK_BACK_15_SECONDS",
        "GO_BACK_15_SECONDS": "SEEK_BACK_15_SECONDS",
        "GO_FORWARD_15_SECONDS": "SEEK_FORWARD_15_SECONDS",
        "SKIP_FORWARD_15_SECONDS": "SEEK_FORWARD_15_SECONDS",
        "SKIP_BACK_15_SECONDS": "SEEK_BACK_15_SECONDS",
        "BACK_15": "SEEK_BACK_15_SECONDS",
        "FORWARD_15": "SEEK_FORWARD_15_SECONDS",
        "REWIND": "SEEK_BACK_15_SECONDS",
        "FAST_FORWARD": "SEEK_FORWARD_15_SECONDS",
        "SET_SPEED_1_5X": "SET_PLAYBACK_SPEED_1POINT5X",
        "SET_SPEED_1_2X": "SET_PLAYBACK_SPEED_1POINT2X",
        "SET_SPEED_2X": "SET_PLAYBACK_SPEED_2X",
        "SET_SPEED": "SET_PLAYBACK_SPEED_1X",
        "SET_PLAYBACK_SPEED_1_POINT_5X": "SET_PLAYBACK_SPEED_1POINT5X",
        "SET_PLAYBACK_SPEED_1_POINT_2X": "SET_PLAYBACK_SPEED_1POINT2X",
        "SET_PLAYBACK_SPEED_1POINT_2X": "SET_PLAYBACK_SPEED_1POINT2X",
        "SET_PLAYBACK_SPEED_1POINT_5X": "SET_PLAYBACK_SPEED_1POINT5X",
        "PLAYBACK_SPEED_NORMAL": "SET_PLAYBACK_SPEED_1X",
        "PLAYBACK_SPEED_FASTER": "SET_PLAYBACK_SPEED_1POINT5X",
        "NORMAL_SPEED": "SET_PLAYBACK_SPEED_1X",
        "OPEN_SETTINGS": "NO_INTENT",
        "TELL_A_JOKE": "NO_INTENT",
        "TELL_ME_A_JOKE": "NO_INTENT",
        "SHUT_OFF": "STOP",
        "TURN_OFF": "STOP",
    ]

    public static let validIntents: Set<String> = [
        "ADD_TO_COLLECTION", "ADD_TO_QUEUE", "BAN_TRACK", "CANCEL_INTERACTION",
        "CLARIFY", "FOLLOW", "HELP", "MORE_LIKE_THIS", "MUTE", "NEXT",
        "NO_INTENT", "OPEN_WEBAPP", "PAUSE", "PLAY", "PLAY_PRESET",
        "PREVIOUS", "REPEAT_OFF", "REPEAT_ON", "REPEAT_ONE", "RESUME",
        "SAVE_TO_PRESET", "SEARCH", "SEEK_BACK_15_SECONDS",
        "SEEK_FORWARD_15_SECONDS", "SET_PLAYBACK_SPEED_1POINT2X",
        "SET_PLAYBACK_SPEED_1POINT5X", "SET_PLAYBACK_SPEED_1X",
        "SET_PLAYBACK_SPEED_2X", "SHOW", "SHOW_MY_LIBRARY",
        "SHOW_MY_NEW_EPISODES", "SHOW_MY_PRESETS", "SHOW_MY_SAVED_EPISODES",
        "SHOW_MY_SONGS", "SHOW_THE_QUEUE", "SHOW_THIS_ARTIST", "SHUFFLE_OFF",
        "SHUFFLE_ON", "STOP", "THUMBS_UP", "TRANSFER_PLAYBACK", "UNMUTE",
        "VOLUME_ABSOLUTE", "VOLUME_DOWN", "VOLUME_UP", "WEBAPP_INTENT",
        "WHATS_PLAYING",
    ]

    public enum SnapReason: Equatable {
        case exact
        case synonym
        case edit(Int)
        case noMatch
    }

    /// Snap a raw intent string to the nearest valid intent.
    public static func snapIntent(_ raw: String?, editMax: Int = 3) -> (String?, SnapReason) {
        guard let raw, !raw.isEmpty else { return (nil, .noMatch) }
        if validIntents.contains(raw) { return (raw, .exact) }
        let rawU = raw.uppercased().trimmingCharacters(in: .whitespaces)
        if let mapped = synonymMap[rawU] { return (mapped, .synonym) }
        if validIntents.contains(rawU) { return (rawU, .exact) }

        var best: (Int, String?) = (editMax + 1, nil)
        for valid in validIntents {
            let d = levenshtein(rawU, valid)
            if d < best.0 { best = (d, valid) }
        }
        if let target = best.1, best.0 <= editMax {
            return (target, .edit(best.0))
        }
        return (nil, .noMatch)
    }

    /// Apply intent snap + WEBAPP_INTENT raw_query filler-strip to a parsed
    /// prediction. Returns `(snapped, reason)` mirroring the Python contract.
    /// `nil` snapped means dispatch should fall back (no reasonable target).
    public static func snapPrediction(_ pred: NluPrediction?) -> (NluPrediction?, SnapReason) {
        guard let pred else { return (nil, .noMatch) }
        let (snapped, reason) = snapIntent(pred.intent)
        guard let target = snapped else { return (nil, reason) }

        var next = pred
        if target != pred.intent {
            next.intent = target
        }
        if target == "WEBAPP_INTENT" {
            if let raw = next.slots.rawQuery {
                let canonical = NluFillerStrip.strip(raw)
                if !canonical.isEmpty, canonical != raw {
                    next.slots.rawQuery = canonical
                }
            }
        }
        return (next, reason)
    }

    /// Iterative Levenshtein distance.
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
