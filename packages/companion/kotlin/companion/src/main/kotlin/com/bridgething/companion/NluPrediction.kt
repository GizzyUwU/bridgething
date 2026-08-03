package com.bridgething.companion

import com.bridgething.schema.NluAlternate
import com.bridgething.schema.NluAmount
import com.bridgething.schema.NluDirection
import com.bridgething.schema.NluPhoneAction
import com.bridgething.schema.NluPlaybackSpeed
import com.bridgething.schema.NluPopularityFilter
import com.bridgething.schema.NluRepeatMode
import com.bridgething.schema.NluResolvedIntent
import com.bridgething.schema.NluScope
import com.bridgething.schema.NluSlots
import com.bridgething.schema.NluTargetType
import com.bridgething.schema.NluView

data class NluPrediction(
    var intent: String,
    var transcript: String,
    var slots: NluMutableSlots = NluMutableSlots(),
    var alternates: List<NluAlternate>? = null,
) {
    fun toWire(): NluResolvedIntent =
        NluResolvedIntent(
            intent = intent,
            slots = slots.toWire(),
            transcript = transcript,
            alternates = alternates,
        )

    companion object {
        fun fromWire(r: NluResolvedIntent): NluPrediction =
            NluPrediction(
                intent = r.intent,
                transcript = r.transcript,
                slots = NluMutableSlots.fromWire(r.slots),
                alternates = r.alternates,
            )
    }
}

data class NluMutableSlots(
    var target: String? = null,
    var targetType: NluTargetType? = null,
    var playlist: String? = null,
    var genre: String? = null,
    var mood: String? = null,
    var era: String? = null,
    var popularityFilter: NluPopularityFilter? = null,
    var position: UInt? = null,
    var count: UInt? = null,
    var scope: NluScope? = null,
    var enabled: Boolean? = null,
    var mute: Boolean? = null,
    var repeatMode: NluRepeatMode? = null,
    var seconds: Int? = null,
    var speed: NluPlaybackSpeed? = null,
    var direction: NluDirection? = null,
    var amount: NluAmount? = null,
    var level: UInt? = null,
    var preset: String? = null,
    var view: NluView? = null,
    var phoneAction: NluPhoneAction? = null,
    var webappName: String? = null,
    var uri: String? = null,
    var contextUri: String? = null,
) {
    fun toWire(): NluSlots =
        NluSlots(
            target = target,
            targetType = targetType,
            playlist = playlist,
            genre = genre,
            mood = mood,
            era = era,
            popularityFilter = popularityFilter,
            position = position,
            count = count,
            scope = scope,
            enabled = enabled,
            mute = mute,
            repeatMode = repeatMode,
            seconds = seconds,
            speed = speed,
            direction = direction,
            amount = amount,
            level = level,
            preset = preset,
            view = view,
            phoneAction = phoneAction,
            webappName = webappName,
            uri = uri,
            contextUri = contextUri,
        )

    companion object {
        fun fromWire(s: NluSlots): NluMutableSlots =
            NluMutableSlots(
                target = s.target,
                targetType = s.targetType,
                playlist = s.playlist,
                genre = s.genre,
                mood = s.mood,
                era = s.era,
                popularityFilter = s.popularityFilter,
                position = s.position,
                count = s.count,
                scope = s.scope,
                enabled = s.enabled,
                mute = s.mute,
                repeatMode = s.repeatMode,
                seconds = s.seconds,
                speed = s.speed,
                direction = s.direction,
                amount = s.amount,
                level = s.level,
                preset = s.preset,
                view = s.view,
                phoneAction = s.phoneAction,
                webappName = s.webappName,
                uri = s.uri,
                contextUri = s.contextUri,
            )
    }
}
