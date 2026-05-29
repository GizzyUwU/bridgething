package dev.bridgething.spotify

class SpotifyUri private constructor(
    val namespace: String,
    val kind: Kind,
    val id: String,
    private val collectionStr: String?,
) {
    fun string(): String {
        val namespacePortion = if (namespace.isNotEmpty()) "$namespace:" else ""
        return collectionStr ?: "$namespacePortion${kind.raw}:$id"
    }

    fun urlEncodedString(): String {
        val namespacePortion = if (namespace.isNotEmpty()) "$namespace%3A" else ""
        return collectionStr?.replace(":", "%3A") ?: "$namespacePortion${kind.raw}%3A$id"
    }

    enum class Kind(val raw: String) {
        AD("ad"),
        ALBUM("album"),
        APPLICATION("application"),
        ARTIST("artist"),
        ARTIST_TOPLIST("artist-toplist"),
        AUDIOBOOK("audiobook"),
        AUDIO_FILE("audiofile"),
        COLLECTION("collection"),
        CONCERT("concert"),
        CONTEXT_GROUP("context-group"),
        DAILY_MIX("dailymix"),
        EMPTY("empty"),
        EPISODE("episode"),
        FACEBOOK("facebook"),
        FOLDER("folder"),
        FOLLOWERS("followers"),
        FOLLOWING("following"),
        IMAGE("image"),
        INBOX("inbox"),
        INTERRUPTION("interruption"),
        LIBRARY("library"),
        LIVE("live"),
        LOCAL_TRACK("local"),
        LOCAL_ALBUM("local-album"),
        LOCAL_ARTIST("local-artist"),
        MOSAIC("mosaic"),
        PLAYLIST("playlist"),
        PLAYLIST_V2("playlist-v2"),
        PROFILE("profile"),
        PUBLISHED_ROOTLIST("published-rootlist"),
        RADIO("radio"),
        ROOTLIST("rootlist"),
        SEARCH("search"),
        SHOW("show"),
        SOCIAL_SESSION("socialsession"),
        SPECIAL("special"),
        STARRED("starred"),
        STATION("station"),
        TEMP_PLAYLIST("temp-playlist"),
        TOPLIST("toplist"),
        TRACK("track"),
        TRACKSET("trackset"),
        USER_TOPLIST("user-toplist"),
        USER_TOP_TRACKS("user-top-track"),
        YOUR_EPISODES("_your-episodes"),
        DEVICE("_device"),
        CONNECT_DEVICES("_connectDevices"),
        ;

        companion object {
            fun fromRaw(raw: String): Kind? = entries.firstOrNull { it.raw == raw }
        }
    }

    object Static {
        const val YOUR_EPISODES = "spotify:playlist:37i9dQZF1FgnTBfUlzkeKt"
        const val DJ = "spotify:playlist:37i9dQZF1EYkqdzj48dyYq"
    }

    companion object {
        private val uriToKindRemappings: List<Pair<String, Kind>> = listOf(
            Static.YOUR_EPISODES to Kind.YOUR_EPISODES,
            "spotify:user:CONNECTOR:collection:DEVICE_SEL:::" to Kind.CONNECT_DEVICES,
        )

        fun parse(uri: String): SpotifyUri? {
            // liked-songs collections look like spotify:user:userid:collection.
            if (uri.startsWith("spotify:user:") && uri.endsWith(":collection")) {
                return SpotifyUri(namespace = "spotify", kind = Kind.COLLECTION, id = "", collectionStr = uri)
            }

            val parts = uri.split(":")
            if (parts.size != 3) return null

            val kind = Kind.fromRaw(parts[1]) ?: return null

            val remappedKind = uriToKindRemappings.firstOrNull { (prefix, _) -> uri.startsWith(prefix) }?.second

            return SpotifyUri(
                namespace = parts[0],
                kind = remappedKind ?: kind,
                id = parts[2],
                collectionStr = null,
            )
        }

        fun build(namespace: String, kind: Kind, id: String): SpotifyUri =
            if (kind == Kind.COLLECTION) {
                SpotifyUri(namespace = namespace, kind = kind, id = "", collectionStr = "spotify:user:$id:collection")
            } else {
                SpotifyUri(namespace = namespace, kind = kind, id = id, collectionStr = null)
            }

        fun build(kind: Kind, id: String): SpotifyUri = build("spotify", kind, id)
    }
}
