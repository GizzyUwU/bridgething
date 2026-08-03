package com.bridgething.nlukit

import com.bridgething.schema.NluAmount
import com.bridgething.schema.NluDirection
import com.bridgething.schema.NluPhoneAction
import com.bridgething.schema.NluPlaybackSpeed
import com.bridgething.schema.NluPopularityFilter
import com.bridgething.schema.NluRepeatMode
import com.bridgething.schema.NluScope
import com.bridgething.schema.NluSlots
import com.bridgething.schema.NluTargetType
import com.bridgething.schema.NluView
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.DisplayName
import org.junit.jupiter.api.Test

class NluSlotMappingTest {
    @Test
    @DisplayName("span slots pass through verbatim")
    fun spanSlots() {
        val out = NluSlotMapping.apply(
            slots(
                "target" to "héroes by beyoncé",
                "playlist" to "workout",
                "genre" to "jazz",
                "mood" to "chill",
                "era" to "80s",
                "webapp_name" to "weather",
                "preset" to "2",
            ),
        )
        assertEquals("héroes by beyoncé", out.target)
        assertEquals("workout", out.playlist)
        assertEquals("jazz", out.genre)
        assertEquals("chill", out.mood)
        assertEquals("80s", out.era)
        assertEquals("weather", out.webappName)
        assertEquals("2", out.preset)
    }

    @Test
    @DisplayName("snake_case yaml tokens resolve to the wire enums")
    fun closedSlots() {
        val out = NluSlotMapping.apply(
            slots(
                "target_type" to "album",
                "popularity_filter" to "top_5",
                "scope" to "previous_track",
                "view" to "now_playing",
                "repeat_mode" to "one",
                "direction" to "up",
                "amount" to "large",
                "phone_action" to "answer",
            ),
        )
        assertEquals(NluTargetType.Album, out.targetType)
        assertEquals(NluPopularityFilter.Top5, out.popularityFilter)
        assertEquals(NluScope.PreviousTrack, out.scope)
        assertEquals(NluView.NowPlaying, out.view)
        assertEquals(NluRepeatMode.One, out.repeatMode)
        assertEquals(NluDirection.Up, out.direction)
        assertEquals(NluAmount.Large, out.amount)
        assertEquals(NluPhoneAction.Answer, out.phoneAction)
    }

    @Test
    @DisplayName("playback speed is matched without case folding")
    fun speedSlot() {
        assertEquals(NluPlaybackSpeed.OnePointFive, NluSlotMapping.apply(slots("speed" to "1.5")).speed)
        assertEquals(NluPlaybackSpeed.Two, NluSlotMapping.apply(slots("speed" to "2")).speed)
    }

    @Test
    @DisplayName("python-stringified booleans decode either case")
    fun boolSlots() {
        assertEquals(true, NluSlotMapping.apply(slots("enabled" to "True")).enabled)
        assertEquals(false, NluSlotMapping.apply(slots("enabled" to "false")).enabled)
        assertEquals(true, NluSlotMapping.apply(slots("mute" to "true")).mute)
        assertNull(NluSlotMapping.apply(slots("enabled" to "maybe")).enabled)
    }

    @Test
    @DisplayName("numeric slots parse and reject non-numbers")
    fun numericSlots() {
        val out = NluSlotMapping.apply(
            slots("count" to "2", "position" to "3", "level" to "4", "seconds" to "-30"),
        )
        assertEquals(2u, out.count)
        assertEquals(3u, out.position)
        assertEquals(4u, out.level)
        assertEquals(-30, out.seconds)
        assertNull(NluSlotMapping.apply(slots("count" to "a few")).count)
        assertNull(NluSlotMapping.apply(slots("level" to "-1")).level)
    }

    @Test
    @DisplayName("values outside a wire enum are dropped rather than guessed")
    fun unknownValuesDropped() {
        assertNull(NluSlotMapping.apply(slots("view" to "cover_flow")).view)
        assertNull(NluSlotMapping.apply(slots("target_type" to "audiobook")).targetType)
    }

    @Test
    @DisplayName("slot names the wire shape does not carry are ignored")
    fun unknownNamesIgnored() {
        assertEquals(NluSlots(), NluSlotMapping.apply(slots("nonesuch" to "value")))
    }

    @Test
    @DisplayName("camel folding matches the generated spellings")
    fun camelFolding() {
        assertEquals("nowPlaying", NluSlotMapping.camel("now_playing"))
        assertEquals("top5", NluSlotMapping.camel("top_5"))
        assertEquals("previousTrack", NluSlotMapping.camel("previous_track"))
        assertEquals("album", NluSlotMapping.camel("album"))
    }
}
