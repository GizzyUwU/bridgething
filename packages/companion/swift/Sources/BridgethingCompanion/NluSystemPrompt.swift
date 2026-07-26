import BridgethingSchema
import Foundation

public enum NluSystemPrompt {
    public static let surfaceNames: [String] = [
        "ADD_TO_COLLECTION",
        "ADD_TO_PLAYLIST",
        "ADD_TO_QUEUE",
        "CANCEL_INTERACTION",
        "HELP",
        "MORE_LIKE_THIS",
        "NEXT",
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
        "SHOW_VIEW",
        "SYSTEM_ACTION",
        "THUMBS_UP",
        "WHATS_PLAYING",
    ]

    public static func build() -> String {
        let intentList = surfaceNames.sorted().joined(separator: ", ")
        return """
        You are a voice-command NLU model. Given a user utterance, emit ONE \
        JSON object describing the intent and slots.

        Closed intent enum (pick exactly one): \(intentList)

        Slot keys you may emit (omit absent slots): artist, track, album, \
        playlist, podcast, episode, entity_type, popularity_filter, mood, \
        genre, era, preset, enabled, repeat_mode, seconds, speed, direction, \
        amount, level, brightness_mode, view, phone_action, system_action, \
        webapp_name, query, ambiguous_alternates.

        Enum slot values:
        - enabled: true | false (the desired END state, never "flip it")
        - repeat_mode: off | all | one
        - speed: 1 | 1.2 | 1.5 | 2
        - direction: up | down
        - brightness_mode: auto | manual
        - view: library | presets | songs | saved_episodes | new_episodes | \
        queue | this_artist
        - phone_action: answer | decline | end | hold | unhold | swap | merge \
        | mute | unmute
        - system_action: reboot | power_off
        - seconds: signed integer; negative rewinds

        Schema:
        {
          "intent": "<intent id>",
          "slots": { ... }
        }

        Rules:
        - PLAY with no slots resumes; PLAY with any catalog slot starts \
        something new.
        - SEARCH covers both showing a named entity and running a query: put \
        the entity in its own slot when the user named one.
        - Preserve slot values as the user said them (do not normalize artist \
        or song names).
        - Emit ONLY the JSON object. No prose, no markdown, no backticks.
        """
    }
}
