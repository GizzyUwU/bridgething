package dev.bridgething.spotify

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

class PlayerResource(private val client: SpotinyClient) {
    private fun deviceIdStr(deviceId: String?): String =
        if (!deviceId.isNullOrEmpty()) "device_id=$deviceId" else ""

    suspend fun transferPlayback(deviceId: String) {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player"
        val body = """{"device_ids": ["$deviceId"]}"""
        client.http.put(urlStr, body)
    }

    suspend fun getAvailableDevices(): List<Device>? {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/devices"
        val data = client.http.getJson<AvailableDevices>(urlStr) ?: return null
        return data.devices
    }

    suspend fun getPlaybackState(): PlayerState? {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player?additional_types=track,episode"
        return client.http.getJson<PlayerState>(urlStr)
    }

    suspend fun getRecentlyPlayed(limit: Int = 20): List<Track> {
        val clamped = limit.coerceIn(1, 50)
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/recently-played?limit=$clamped"
        val page = client.http.getJson<RecentlyPlayedResponse>(urlStr) ?: return emptyList()
        return page.items.mapNotNull { it.track }
    }

    suspend fun resume(deviceId: String? = "") {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/play?${deviceIdStr(deviceId)}"
        client.http.put(urlStr)
    }

    suspend fun pause(deviceId: String? = "") {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/pause?${deviceIdStr(deviceId)}"
        client.http.put(urlStr)
    }

    suspend fun skipNext(deviceId: String? = "") {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/next?${deviceIdStr(deviceId)}"
        client.http.post(urlStr)
    }

    suspend fun skipPrevious(deviceId: String? = "") {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/previous?${deviceIdStr(deviceId)}"
        client.http.post(urlStr)
    }

    suspend fun seek(positionMs: Int, deviceId: String? = "") {
        val clamped = maxOf(positionMs, 0)
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/seek?position_ms=$clamped&${deviceIdStr(deviceId)}"
        client.http.put(urlStr)
    }

    suspend fun setPlaybackVolume(volume: Int, deviceId: String? = "") {
        val clamped = volume.coerceIn(0, 100)
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/volume?volume_percent=$clamped&${deviceIdStr(deviceId)}"
        client.http.put(urlStr)
    }

    suspend fun play(uri: SpotifyUri?, skipToUri: SpotifyUri? = null, deviceId: String? = "") {
        if (uri == null) return
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/play?${deviceIdStr(deviceId)}"
        val body = when (uri.kind) {
            SpotifyUri.Kind.TRACK, SpotifyUri.Kind.EPISODE ->
                """{"uris": ["${uri.string()}"]}"""
            SpotifyUri.Kind.ARTIST, SpotifyUri.Kind.ALBUM, SpotifyUri.Kind.PLAYLIST,
            SpotifyUri.Kind.SHOW, SpotifyUri.Kind.COLLECTION ->
                if (skipToUri != null) {
                    """{"context_uri": "${uri.string()}", "offset": {"uri": "${skipToUri.string()}"}}"""
                } else {
                    """{"context_uri": "${uri.string()}"}"""
                }
            else -> ""
        }
        client.http.put(urlStr, body)
    }

    suspend fun setRepeatMode(mode: RepeatMode, deviceId: String? = "") {
        val state = when (mode) {
            RepeatMode.OFF -> "off"
            RepeatMode.TRACK -> "track"
            RepeatMode.CONTEXT -> "context"
        }
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/repeat?state=$state&${deviceIdStr(deviceId)}"
        client.http.put(urlStr)
    }

    suspend fun setShuffle(enabled: Boolean, deviceId: String? = "") {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/shuffle?state=${if (enabled) "true" else "false"}&${deviceIdStr(deviceId)}"
        client.http.put(urlStr)
    }

    suspend fun getQueue(): PlayerQueue? {
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/queue"
        return client.http.getJson<PlayerQueue>(urlStr)
    }

    suspend fun addItemToQueue(uri: SpotifyUri?, deviceId: String? = "") {
        if (uri == null) return
        val urlStr = "$SPOTIFY_API_BASE/v1/me/player/queue?uri=${uri.urlEncodedString()}&${deviceIdStr(deviceId)}"
        client.http.post(urlStr)
    }
}

@Serializable
private data class RecentlyPlayedResponse(
    @Serializable(with = LossyListSerializer::class) val items: List<Row> = emptyList(),
) {
    @Serializable
    data class Row(val track: Track? = null)
}
