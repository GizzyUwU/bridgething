import Foundation

public enum NluIntentCatalog {
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

    public static let noIntent = "NO_INTENT"
    public static let clarify = "CLARIFY"

    public static func name(at index: Int) -> String? {
        surfaceNames.indices.contains(index) ? surfaceNames[index] : nil
    }

    public static func contains(_ name: String) -> Bool {
        surfaceNames.contains(name)
    }
}
