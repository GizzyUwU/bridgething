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
import uniffi.nlu.SlotValue

object NluSlotMapping {
    fun apply(slots: List<SlotValue>): NluSlots =
        slots.fold(NluSlots()) { out, slot ->
            val v = slot.value
            when (slot.name) {
                "target" -> out.copy(target = v)
                "playlist" -> out.copy(playlist = v)
                "genre" -> out.copy(genre = v)
                "mood" -> out.copy(mood = v)
                "era" -> out.copy(era = v)
                "webapp_name" -> out.copy(webappName = v)
                "preset" -> out.copy(preset = v)
                "target_type" -> out.copy(targetType = enum(NluTargetType.entries, camel(v)) { it.string })
                "popularity_filter" -> out.copy(popularityFilter = enum(NluPopularityFilter.entries, camel(v)) { it.string })
                "scope" -> out.copy(scope = enum(NluScope.entries, camel(v)) { it.string })
                "view" -> out.copy(view = enum(NluView.entries, camel(v)) { it.string })
                "repeat_mode" -> out.copy(repeatMode = enum(NluRepeatMode.entries, camel(v)) { it.string })
                "speed" -> out.copy(speed = enum(NluPlaybackSpeed.entries, v) { it.string })
                "direction" -> out.copy(direction = enum(NluDirection.entries, camel(v)) { it.string })
                "amount" -> out.copy(amount = enum(NluAmount.entries, camel(v)) { it.string })
                "phone_action" -> out.copy(phoneAction = enum(NluPhoneAction.entries, camel(v)) { it.string })
                "enabled" -> out.copy(enabled = bool(v))
                "mute" -> out.copy(mute = bool(v))
                "count" -> out.copy(count = v.toUIntOrNull())
                "position" -> out.copy(position = v.toUIntOrNull())
                "level" -> out.copy(level = v.toUIntOrNull())
                "seconds" -> out.copy(seconds = v.toIntOrNull())
                else -> out
            }
        }

    fun camel(token: String): String {
        val parts = token.split("_")
        if (parts.isEmpty()) return token
        return parts.first() + parts.drop(1).joinToString("") { it.replaceFirstChar(Char::uppercaseChar) }
    }

    fun bool(token: String): Boolean? = when (token.lowercase()) {
        "true" -> true
        "false" -> false
        else -> null
    }

    private fun <T> enum(entries: List<T>, token: String, wire: (T) -> String): T? =
        entries.firstOrNull { wire(it) == token }
}
