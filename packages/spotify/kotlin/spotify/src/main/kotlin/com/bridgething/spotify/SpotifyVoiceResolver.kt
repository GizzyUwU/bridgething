package com.bridgething.spotify

import com.bridgething.companion.NluMutableSlots
import com.bridgething.companion.NluPrediction
import com.bridgething.companion.VoiceCatalogResolving
import com.bridgething.schema.NluPopularityFilter
import com.bridgething.schema.NluTargetType
import uniffi.spotify.SpotifyClientInterface
import uniffi.spotify.VoicePopularity
import uniffi.spotify.VoiceResolveRequest
import uniffi.spotify.VoiceTargetKind

class SpotifyVoiceResolver(private val client: SpotifyClientInterface) : VoiceCatalogResolving {
    override suspend fun decorate(prediction: NluPrediction): NluPrediction {
        if (!isCatalogIntent(prediction.intent)) return prediction
        val req = request(prediction.slots) ?: return prediction
        val resolved = client.resolveVoice(req)
        return prediction.copy(
            slots = prediction.slots.copy(uri = resolved.uri, contextUri = resolved.contextUri),
        )
    }

    private companion object {
        fun isCatalogIntent(intent: String): Boolean = when (intent) {
            "PLAY", "ADD_TO_QUEUE", "ADD_TO_PLAYLIST", "SEARCH", "THUMBS_UP" -> true
            else -> false
        }

        fun request(slots: NluMutableSlots): VoiceResolveRequest? {
            val target = text(slots.target)
            val mood = text(slots.mood)
            val genre = text(slots.genre)
            val era = text(slots.era)
            val popularity = popularity(slots.popularityFilter)
            if (target == null && mood == null && genre == null && era == null &&
                slots.position == null && popularity == null
            ) {
                return null
            }
            return VoiceResolveRequest(
                target = target,
                targetType = kind(slots.targetType),
                mood = mood,
                genre = genre,
                era = era,
                popularityFilter = popularity,
                position = slots.position,
            )
        }

        fun text(value: String?): String? = value?.trim()?.ifEmpty { null }

        fun kind(type: NluTargetType?): VoiceTargetKind? = when (type) {
            NluTargetType.Artist -> VoiceTargetKind.ARTIST
            NluTargetType.Track -> VoiceTargetKind.TRACK
            NluTargetType.Album -> VoiceTargetKind.ALBUM
            NluTargetType.Playlist -> VoiceTargetKind.PLAYLIST
            NluTargetType.Podcast -> VoiceTargetKind.SHOW
            NluTargetType.Episode -> VoiceTargetKind.EPISODE
            NluTargetType.Station -> VoiceTargetKind.STATION
            null -> null
        }

        fun popularity(filter: NluPopularityFilter?): VoicePopularity? = when (filter) {
            NluPopularityFilter.Top5 -> VoicePopularity.TOP5
            NluPopularityFilter.Top10 -> VoicePopularity.TOP10
            NluPopularityFilter.Popular -> VoicePopularity.POPULAR
            NluPopularityFilter.Recent -> VoicePopularity.RECENT
            NluPopularityFilter.New -> VoicePopularity.NEW
            NluPopularityFilter.Random -> VoicePopularity.RANDOM
            null -> null
        }
    }
}
