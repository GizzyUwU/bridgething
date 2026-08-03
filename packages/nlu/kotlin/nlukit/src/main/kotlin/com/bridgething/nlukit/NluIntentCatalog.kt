package com.bridgething.nlukit

object NluIntentCatalog {
    val surfaceNames: List<String> = listOf(
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
        "PRESET_PLAY",
        "PRESET_SAVE",
        "PREVIOUS",
        "SEARCH",
        "SEEK_RELATIVE",
        "SET_DISCOVERABLE",
        "SET_PLAYBACK_SPEED",
        "SET_REPEAT",
        "SET_SHUFFLE",
        "SET_VOLUME",
        "SHOW_VIEW",
        "THUMBS_UP",
    )

    const val NO_INTENT = "NO_INTENT"
    const val CLARIFY = "CLARIFY"

    fun name(at: Int): String? = surfaceNames.getOrNull(at)

    fun contains(name: String): Boolean = surfaceNames.contains(name)
}
