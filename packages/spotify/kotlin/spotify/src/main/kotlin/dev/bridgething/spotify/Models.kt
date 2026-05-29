package dev.bridgething.spotify

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** list serializer that silently drops elements that fail to decode rather than failing the whole array. */
class LossyListSerializer<T>(private val elementSerializer: KSerializer<T>) : KSerializer<List<T>> {
    private val delegate = ListSerializer(elementSerializer)
    override val descriptor: SerialDescriptor = delegate.descriptor
    override fun serialize(encoder: Encoder, value: List<T>) = delegate.serialize(encoder, value)
    override fun deserialize(decoder: Decoder): List<T> {
        val input = decoder as? JsonDecoder ?: return delegate.deserialize(decoder)
        val array = input.decodeJsonElement() as? JsonArray ?: return emptyList()
        return array.mapNotNull { element ->
            runCatching { input.json.decodeFromJsonElement(elementSerializer, element) }.getOrNull()
        }
    }
}

@Serializable
data class SpotifyImage(
    val url: String? = null,
    val height: Int? = null,
    val width: Int? = null,
)

class SpotifyImageURLs(images: List<SpotifyImage>?) {
    val small: String
    val medium: String
    val large: String

    init {
        if (images == null || images.isEmpty()) {
            small = ""
            medium = ""
            large = ""
        } else {
            small = images.last().url ?: ""
            medium = images[images.size / 2].url ?: ""
            large = images.first().url ?: ""
        }
    }
}

@Serializable
data class Track(
    val id: String = "",
    val uri: String = "",
    val name: String = "",
    val explicit: Boolean = false,
    @SerialName("duration_ms") val durationMs: Int = 0,
    val album: Album? = null,
    val artists: List<Artist> = emptyList(),
) {
    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(album?.images ?: emptyList())

    val subtitle: String
        get() = artists.joinToString(", ") { it.name }
}

@Serializable
data class Album(
    val id: String = "",
    val uri: String = "",
    val name: String = "",
    val artists: List<Artist> = emptyList(),
    val images: List<SpotifyImage>? = null,
) {
    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(images)
}

@Serializable(with = ArtistSerializer::class)
data class Artist(
    val id: String = "",
    val uri: String = "",
    val name: String = "",
    val type: String = "",
    val images: List<SpotifyImage>? = null,
) {
    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(images)
}

@Serializable
private data class ArtistRaw(
    val id: String = "",
    val uri: String = "",
    val name: String = "",
    val type: String = "",
    val images: List<SpotifyImage>? = null,
)

object ArtistSerializer : KSerializer<Artist> {
    override val descriptor: SerialDescriptor = ArtistRaw.serializer().descriptor

    override fun deserialize(decoder: Decoder): Artist {
        val raw = ArtistRaw.serializer().deserialize(decoder)
        // spotify returns the podcast show name as the artist type in playlists; remap show entries.
        return if (SpotifyUri.parse(raw.uri)?.kind == SpotifyUri.Kind.SHOW) {
            Artist(id = raw.id, uri = raw.uri, name = raw.type, type = SpotifyUri.Kind.SHOW.raw, images = raw.images)
        } else {
            Artist(id = raw.id, uri = raw.uri, name = raw.name, type = raw.type, images = raw.images)
        }
    }

    override fun serialize(encoder: Encoder, value: Artist) {
        ArtistRaw.serializer().serialize(
            encoder,
            ArtistRaw(value.id, value.uri, value.name, value.type, value.images),
        )
    }
}

@Serializable
data class Playlist(
    val id: String = "",
    val uri: String = "",
    val name: String = "",
    val description: String = "",
    val images: List<SpotifyImage>? = null,
) {
    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(images)
}

@Serializable
data class PlaylistItem(
    val type: String = "",
    val id: String = "",
    val uri: String = "",
    val name: String? = null,
    val explicit: Boolean = false,
    @SerialName("duration_ms") val durationMs: Int = 0,
    val artists: List<Artist> = emptyList(),
    val album: Album? = null,
    val images: List<SpotifyImage>? = null,
) {
    val subtitle: String
        get() = artists.joinToString(", ") { it.name }

    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(album?.images ?: emptyList())
}

@Serializable
data class Show(
    val id: String = "",
    val uri: String = "",
    val name: String = "",
    val description: String = "",
    val publisher: String = "",
    val explicit: Boolean = false,
    val images: List<SpotifyImage>? = null,
) {
    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(images)
}

@Serializable
data class Episode(
    val id: String = "",
    val uri: String = "",
    val name: String = "",
    val description: String = "",
    val explicit: Boolean = false,
    @SerialName("duration_ms") val durationMs: Int = 0,
    @SerialName("release_date") val releaseDate: String = "",
    val images: List<SpotifyImage>? = null,
    val show: Show? = null,
    val artists: List<Artist>? = null,
    @SerialName("resume_point") val resumePoint: ResumePoint? = null,
) {
    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(images)

    @Serializable
    data class ResumePoint(
        @SerialName("fully_played") val fullyPlayed: Boolean = false,
        @SerialName("resume_position_ms") val resumePositionMs: Int = 0,
    )
}

@Serializable
data class User(
    val id: String = "",
    val uri: String = "",
    @SerialName("display_name") val displayName: String = "",
    val product: String = "",
    val images: List<SpotifyImage>? = null,
) {
    val imageUrl: SpotifyImageURLs
        get() = SpotifyImageURLs(images)
}

@Serializable
enum class RepeatMode {
    @SerialName("off") OFF,
    @SerialName("track") TRACK,
    @SerialName("context") CONTEXT,
}

@Serializable
data class Device(
    val id: String = "",
    @SerialName("is_active") val isActive: Boolean = false,
    @SerialName("is_private_session") val isPrivateSession: Boolean = false,
    @SerialName("is_restricted") val isRestricted: Boolean = false,
    val name: String = "",
    val type: String = "",
    @SerialName("volume_percent") val volumePercent: Int = 0,
    @SerialName("supports_volume") val supportsVolume: Boolean = false,
)

@Serializable
data class AvailableDevices(
    val devices: List<Device> = emptyList(),
)

@Serializable
data class PlayerContext(
    @SerialName("external_urls") val externalUrls: ExternalUrls? = null,
    val href: String = "",
    val type: String = "",
    val uri: String = "",
) {
    @Serializable
    data class ExternalUrls(
        val spotify: String = "",
    )
}

@Serializable(with = PlayerItemSerializer::class)
sealed class PlayerItem {
    data class TrackItem(val track: Track) : PlayerItem()
    data class EpisodeItem(val episode: Episode) : PlayerItem()

    val name: String
        get() = when (this) {
            is TrackItem -> track.name
            is EpisodeItem -> episode.name
        }

    val subtitle: String
        get() = when (this) {
            is TrackItem -> track.subtitle
            is EpisodeItem -> episode.show?.name ?: episode.show?.publisher ?: episode.description
        }

    val uri: String
        get() = when (this) {
            is TrackItem -> track.uri
            is EpisodeItem -> episode.uri
        }

    val type: String
        get() = when (this) {
            is TrackItem -> SpotifyUri.Kind.TRACK.raw
            is EpisodeItem -> SpotifyUri.Kind.EPISODE.raw
        }

    val id: String
        get() = when (this) {
            is TrackItem -> track.id
            is EpisodeItem -> episode.id
        }

    val explicit: Boolean
        get() = when (this) {
            is TrackItem -> track.explicit
            is EpisodeItem -> episode.explicit
        }

    val durationMs: Int
        get() = when (this) {
            is TrackItem -> track.durationMs
            is EpisodeItem -> episode.durationMs
        }

    val artists: List<Artist>
        get() = when (this) {
            is TrackItem -> track.artists
            is EpisodeItem -> episode.artists ?: emptyList()
        }

    val imageUrl: SpotifyImageURLs
        get() = when (this) {
            is TrackItem -> SpotifyImageURLs(track.album?.images ?: emptyList())
            is EpisodeItem -> SpotifyImageURLs(episode.images)
        }
}

object PlayerItemSerializer : KSerializer<PlayerItem> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("PlayerItem")

    override fun deserialize(decoder: Decoder): PlayerItem {
        val input = decoder as? JsonDecoder
            ?: error("PlayerItem can only be deserialized from JSON")
        val element = input.decodeJsonElement()
        val obj = element.jsonObject
        val type = obj["type"]?.jsonPrimitive?.content
        return when (type) {
            SpotifyUri.Kind.TRACK.raw ->
                PlayerItem.TrackItem(input.json.decodeFromJsonElement(Track.serializer(), element))
            SpotifyUri.Kind.EPISODE.raw ->
                PlayerItem.EpisodeItem(input.json.decodeFromJsonElement(Episode.serializer(), element))
            else -> error("Unknown queue item type: $type")
        }
    }

    override fun serialize(encoder: Encoder, value: PlayerItem) {
        when (value) {
            is PlayerItem.TrackItem -> Track.serializer().serialize(encoder, value.track)
            is PlayerItem.EpisodeItem -> Episode.serializer().serialize(encoder, value.episode)
        }
    }
}

@Serializable
data class PlayerQueue(
    @SerialName("currently_playing") val currentlyPlaying: PlayerItem? = null,
    @Serializable(with = LossyListSerializer::class) val queue: List<PlayerItem> = emptyList(),
)

@Serializable
data class PlayerState(
    val device: Device = Device(),
    @SerialName("shuffle_state") val shuffleState: Boolean = false,
    @SerialName("repeat_state") val repeatState: RepeatMode = RepeatMode.OFF,
    @SerialName("is_playing") val isPlaying: Boolean = false,
    val timestamp: Long = 0,
    @SerialName("progress_ms") val progressMs: Int = 0,
    val context: PlayerContext? = null,
    val item: PlayerItem? = null,
    @SerialName("currently_playing_type") val currentlyPlayingType: String = "",
    val actions: DisallowsActions? = null,
) {
    @Serializable
    data class DisallowsActions(
        val disallows: Actions? = null,
    )

    @Serializable
    data class Actions(
        @SerialName("interrupting_playback") val interruptingPlayback: Boolean? = null,
        val seeking: Boolean? = null,
        @SerialName("skipping_next") val skippingNext: Boolean? = null,
        @SerialName("skipping_prev") val skippingPrev: Boolean? = null,
        @SerialName("toggling_repeat_context") val togglingRepeatContext: Boolean? = null,
        @SerialName("toggling_shuffle") val togglingShuffle: Boolean? = null,
        @SerialName("toggling_repeat_track") val togglingRepeatTrack: Boolean? = null,
        @SerialName("transferring_playback") val transferringPlayback: Boolean? = null,
    )
}

data class SpotifySearchResults(
    val tracks: List<Track> = emptyList(),
    val albums: List<Album> = emptyList(),
    val artists: List<Artist> = emptyList(),
    val playlists: List<Playlist> = emptyList(),
    val shows: List<Show> = emptyList(),
    val episodes: List<Episode> = emptyList(),
)

@Serializable
data class ItemsResponse<Item>(
    val next: String? = null,
    val total: Int = 0,
    @Serializable(with = LossyListSerializer::class) val items: List<Item> = emptyList(),
)

@Serializable
data class FollowedArtistsResponse(
    val artists: ItemsResponse<Artist> = ItemsResponse(),
)

@Serializable
data class CategoryPlaylistsResponse(
    val playlists: ItemsResponse<Playlist> = ItemsResponse(),
)

@Serializable
data class SearchResponse(
    val tracks: ItemsResponse<Track>? = null,
    val albums: ItemsResponse<Album>? = null,
    val artists: ItemsResponse<Artist>? = null,
    val playlists: ItemsResponse<Playlist>? = null,
    val shows: ItemsResponse<Show>? = null,
    val episodes: ItemsResponse<Episode>? = null,
)

class ItemsPage<Item>(
    private val backing: List<Item>,
    val total: Int,
) {
    val items: List<Item>
        get() = backing

    fun <NewItem> map(transform: (Item) -> NewItem): ItemsPage<NewItem> =
        ItemsPage(backing.map(transform), total)

    fun filter(isIncluded: (Item) -> Boolean): ItemsPage<Item> =
        ItemsPage(backing.filter(isIncluded), total)

    companion object {
        fun <Item> empty(): ItemsPage<Item> = ItemsPage(emptyList(), 0)
    }
}
