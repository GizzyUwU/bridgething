import BridgethingSchema
import Foundation

/// Build the system prompt the LLM stage runs against. Mirrors
/// `nlu/scripts/probe_music_knowledge.build_system_prompt`. The
/// `activeWebapps` block is what makes WEBAPP_INTENT emission context-
/// aware - without it, an utterance like "what's the weather" rationally
/// falls back to NO_INTENT because the model has no signal a weather
/// webapp exists to route to.
public enum NluSystemPrompt {
    /// Closed intent enum the LLM is told to pick from. Kept in sync with
    /// `configs/intents.yaml` and `NluIntentValidator.validIntents`. Surface
    /// names (what the LLM emits) are listed; the snap layer maps any
    /// close-misses back to this catalog.
    public static let surfaceNames: [String] = [
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

    /// One row of the active-extensions block. `id` is the webapp's wire
    /// id (typically the manifest UUID string; human-readable names like
    /// "home-assistant" work too if your dispatcher maps name -> uuid).
    /// `voiceGrammar` is the plain-English description the manifest
    /// declares; used in the prompt verbatim.
    public struct ActiveWebapp: Sendable {
        public let id: String
        public let voiceGrammar: String

        public init(id: String, voiceGrammar: String) {
            self.id = id
            self.voiceGrammar = voiceGrammar
        }
    }

    public static func build(activeWebapps: [ActiveWebapp] = []) -> String {
        let intentList = surfaceNames.sorted().joined(separator: ", ")
        var prompt = """
        You are a voice-command NLU model. Given a user utterance, emit ONE \
        JSON object describing the intent and slots.

        Closed intent enum (pick exactly one): \(intentList)

        Slot keys you may emit (omit absent slots): artist, track, album, \
        playlist, podcast, episode, entity_type, popularity_filter, mood, \
        genre, era, preset, amount, level, webapp_id, webapp_name, raw_query, \
        query, ambiguous_alternates.

        Schema:
        {
          "intent": "<intent id>",
          "slots": { ... },
          "confidence": {"intent": "low|medium|high", "slots": "low|medium|high"}
        }

        Rules:
        - If the utterance is not a music or device-control command, emit \
        intent NO_INTENT with empty slots.
        - If multiple interpretations are plausible, emit CLARIFY.
        - Preserve slot values as the user said them (do not normalize artist \
        or song names).
        - Emit ONLY the JSON object. No prose, no markdown, no backticks.
        """

        if !activeWebapps.isEmpty {
            prompt += "\n\nCurrently active extensions (emit WEBAPP_INTENT with the matching webapp_id when the utterance fits one of these domains; raw_query is the filler-stripped command):"
            for w in activeWebapps {
                prompt += "\n- \(w.id): \(w.voiceGrammar)"
            }
        }

        return prompt
    }
}
