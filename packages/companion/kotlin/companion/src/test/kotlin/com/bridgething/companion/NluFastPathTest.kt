package com.bridgething.companion

import com.bridgething.schema.NluRepeatMode
import com.bridgething.schema.NluView
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

class NluFastPathTest {
    @Test
    fun `bare transport commands match without slots`() {
        assertEquals("PLAY", NluFastPath.match("play")?.intent)
        assertEquals("PAUSE", NluFastPath.match("pause")?.intent)
        assertEquals("NEXT", NluFastPath.match("next song")?.intent)
        assertEquals("SHOW_VIEW", NluFastPath.match("what's playing")?.intent)
    }

    @Test
    fun `never fires on a command carrying content`() {
        for (utterance in listOf(
            "play some jazz",
            "play bohemian rhapsody",
            "play the new album by black country new road",
            "play my liked songs",
            "add this to my dance playlist",
            "what album is this from",
        )) {
            assertNull(NluFastPath.match(utterance), "fast path must not claim: $utterance")
        }
    }

    @Test
    fun `preset selection captures the number`() {
        val hit = NluFastPath.match("play preset 3")
        assertEquals("PRESET_PLAY", hit?.intent)
        assertEquals("3", hit?.slots?.preset)
    }

    @Test
    fun `preset rejects out-of-range and save phrasings`() {
        assertNull(NluFastPath.match("play preset 7"))
        assertEquals("PRESET_SAVE", NluFastPath.match("save preset 2")?.intent)
    }

    @Test
    fun `rule order keeps overlapping repeat phrasings distinct`() {
        assertEquals("SET_REPEAT", NluFastPath.match("repeat this")?.intent)
        assertEquals(NluRepeatMode.One, NluFastPath.match("repeat this")?.slots?.repeatMode)
        assertEquals(NluRepeatMode.All, NluFastPath.match("repeat on")?.slots?.repeatMode)
        assertEquals(NluRepeatMode.Off, NluFastPath.match("repeat off")?.slots?.repeatMode)
    }

    @Test
    fun `unhandled phrasings fall through instead of guessing`() {
        assertNull(NluFastPath.match("repeat one"))
    }
}

class NluFastPathAsrShapeTest {
    @Test
    fun `matches raw recogniser output without pre-normalisation`() {
        assertEquals("PAUSE", NluFastPath.match("Pause.")?.intent)
        assertEquals("NEXT", NluFastPath.match("Next song.")?.intent)
        assertEquals("SHOW_VIEW", NluFastPath.match("What's playing?")?.intent)
    }

    @Test
    fun `politeness, determiners and generic nouns do not hide the command`() {
        val expected = mapOf(
            "Pause music now." to "PAUSE",
            "Could you pause the song?" to "PAUSE",
            "Stop the song from playing." to "PAUSE",
            "Turn off music." to "PAUSE",
            "End this track." to "PAUSE",
            "Skip the track." to "NEXT",
            "Can you skip this song?" to "NEXT",
            "Skip to the next track." to "NEXT",
            "Would you go to the next song please?" to "NEXT",
            "Go back to previous song." to "PREVIOUS",
            "Please play the previous song." to "PREVIOUS",
            "Replay the last song." to "PREVIOUS",
            "Repeat the last song." to "PREVIOUS",
            "shuffle the tracks" to "SET_SHUFFLE",
            "Put this playlist on shuffle." to "SET_SHUFFLE",
        )
        for ((utterance, intent) in expected) {
            assertEquals(intent, NluFastPath.match(utterance)?.intent, "expected $intent for: $utterance")
        }
    }

    @Test
    fun `repeat scope survives the generic-noun strip`() {
        assertEquals(NluRepeatMode.One, NluFastPath.match("Put song on repeat for me.")?.slots?.repeatMode)
        assertEquals(NluRepeatMode.One, NluFastPath.match("Could you play this song in loop?")?.slots?.repeatMode)
        assertEquals(NluRepeatMode.All, NluFastPath.match("Repeat this playlist indefinitely.")?.slots?.repeatMode)
    }

    @Test
    fun `declines anything carrying content, a scope or a second setting`() {
        for (utterance in listOf(
            "Play Pandora on Shuffle for us.",
            "Play more music like this.",
            "Skip the next two songs.",
            "Skip to track 20.",
            "Shuffle for the next five songs.",
            "Play a list of my favorite songs.",
            "Make a playlist with my most listened to tracks.",
            "Exit Spotify",
        )) {
            assertNull(NluFastPath.match(utterance), "fast path must not claim: $utterance")
        }
    }

    @Test
    fun `a collection word blocks the transport rules the generic strip would blind`() {
        for (utterance in listOf(
            "Go to the next playlist.",
            "Play my last playlist.",
            "shuffle play this playlist.",
            "next album",
            "start the next station",
        )) {
            assertNull(NluFastPath.match(utterance), "fast path must not claim: $utterance")
        }
        assertEquals("NEXT", NluFastPath.match("skip this song")?.intent)
    }

    @Test
    fun `mute folds into SET_VOLUME and whats-playing into SHOW_VIEW`() {
        val mute = NluFastPath.match("mute")
        assertEquals("SET_VOLUME", mute?.intent)
        assertEquals(true, mute?.slots?.mute)
        val unmute = NluFastPath.match("unmute")
        assertEquals("SET_VOLUME", unmute?.intent)
        assertEquals(false, unmute?.slots?.mute)
        val whats = NluFastPath.match("what's playing")
        assertEquals("SHOW_VIEW", whats?.intent)
        assertEquals(NluView.NowPlaying, whats?.slots?.view)
    }
}
