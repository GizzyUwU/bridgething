package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.schema.AcceptCallAction
import com.bridgething.schema.CommunicationsState
import com.bridgething.schema.DtmfTone
import com.bridgething.schema.EndCallAction
import com.bridgething.schema.Notification as WireNotification
import com.bridgething.schema.NotificationRemoved
import com.bridgething.schema.NotificationsError
import com.bridgething.schema.PhoneCall
import com.bridgething.schema.PhoneCallEnded
import com.bridgething.schema.PhoneInitiateAction
import com.bridgething.schema.PhoneState
import com.bridgething.schema.RepeatMode
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import java.util.UUID

public interface GeoSource {
    public suspend fun start(gateway: BridgethingGateway)
    public suspend fun stop()

    /**
     * False only when location is definitively unusable, so the announced `available.geo` does not
     * advertise a surface that refuses everything. Defaults true for sources with nothing to check.
     */
    public val canProvideLocation: Boolean get() = true
}

public interface VolumeSource {
    public fun interface Callback {
        public fun onVolumeChanged(level: Float, muted: Boolean)
    }

    public fun start(callback: Callback)
    public fun stop()
    public fun snapshot(): Pair<Float, Boolean>
}

public interface AudioBackend {
    public suspend fun setVolume(level: Float)
    public suspend fun setMute(muted: Boolean)
    public suspend fun volumeUp()
    public suspend fun volumeDown()
    public suspend fun muteToggle()
    public suspend fun speak(id: UUID, text: String, voice: String?, onStart: () -> Unit): Boolean
    public suspend fun cancel(id: UUID)
    public suspend fun cancelAll()
    public suspend fun playEarcon(name: String): Boolean
}

public sealed interface NotificationOutEvent {
    public data class Posted(val notification: WireNotification) : NotificationOutEvent
    public data class Removed(val removed: NotificationRemoved) : NotificationOutEvent
}

public interface NotificationBackend {
    public val events: Flow<NotificationOutEvent>

    public suspend fun invokePositive(id: String): NotificationsError?
    public suspend fun invokeNegative(id: String): NotificationsError?
}

public object NoOpNotificationBackend : NotificationBackend {
    override val events: Flow<NotificationOutEvent> = emptyFlow()
    override suspend fun invokePositive(id: String): NotificationsError? = null
    override suspend fun invokeNegative(id: String): NotificationsError? = null
}

public sealed interface PhoneOutEvent {
    public data class CallStarted(val call: PhoneCall) : PhoneOutEvent
    public data class CallUpdated(val call: PhoneCall) : PhoneOutEvent
    public data class CallEnded(val ended: PhoneCallEnded) : PhoneOutEvent
    public data class Snapshot(val state: PhoneState) : PhoneOutEvent
    public data class Communications(val state: CommunicationsState) : PhoneOutEvent
}

public interface PhoneBackend {
    public val events: Flow<PhoneOutEvent>
    public suspend fun answer(callId: String)
    public suspend fun accept(callId: String, action: AcceptCallAction)
    public suspend fun decline(callId: String)
    public suspend fun end(callId: String)
    public suspend fun endTyped(callId: String, action: EndCallAction)
    public suspend fun hold(callId: String)
    public suspend fun unhold(callId: String)
    public suspend fun initiate(action: PhoneInitiateAction)
    public suspend fun swap()
    public suspend fun merge()
    public suspend fun mute(muted: Boolean)
    public suspend fun dtmf(callId: String?, tone: DtmfTone)
    public suspend fun stateGet(): PhoneState
}

public interface MediaSessionGateway {
    public val isAccessGranted: Boolean

    public fun activeSessions(): List<SystemMediaSession>

    public fun listen(onChanged: () -> Unit): MediaSessionListenHandle
}

public interface MediaSessionListenHandle {
    public fun stop()
}

public interface SystemMediaSession {
    public val packageName: String

    public fun snapshot(): SystemMediaSnapshot?

    public suspend fun art(token: String): SystemMediaArt?

    public fun play()
    public fun pause()
    public fun skipNext()
    public fun skipPrev()
    public fun seekTo(positionMs: Long)
    public fun skipToQueueItem(queueId: Long)
    public fun setShuffle(on: Boolean)
    public fun setRepeat(mode: RepeatMode)
    public fun setSpeed(speed: Float)
    public fun setLiked(liked: Boolean)
}

public data class SystemMediaSnapshot(
    val title: String?,
    val artist: String?,
    val album: String?,
    val durationMs: Long?,
    val positionMs: Long,
    val playing: Boolean,
    val canSeek: Boolean,
    val artToken: String? = null,
    val queue: List<SystemMediaQueueEntry> = emptyList(),
    val activeQueueId: Long? = null,
    val shuffle: Boolean? = null,
    val repeat: RepeatMode? = null,
    val speed: Float? = null,
    val positionAgeMs: Long? = null,
    val liked: Boolean? = null,
    val likeSupported: Boolean = false,
    val queueTitle: String? = null,
)

public data class SystemMediaQueueEntry(
    val queueId: Long,
    val title: String?,
    val subtitle: String?,
    val artToken: String? = null,
)

public data class SystemMediaArt(val bytes: ByteArray, val mime: String) {
    override fun equals(other: Any?): Boolean =
        other is SystemMediaArt && mime == other.mime && bytes.contentEquals(other.bytes)

    override fun hashCode(): Int = 31 * bytes.contentHashCode() + mime.hashCode()
}

public object NoOpMediaSessionGateway : MediaSessionGateway {
    override val isAccessGranted: Boolean = false
    override fun activeSessions(): List<SystemMediaSession> = emptyList()
    override fun listen(onChanged: () -> Unit): MediaSessionListenHandle = object : MediaSessionListenHandle {
        override fun stop() {}
    }
}

public object NoOpPhoneBackend : PhoneBackend {
    override val events: Flow<PhoneOutEvent> = emptyFlow()
    override suspend fun answer(callId: String) {}
    override suspend fun accept(callId: String, action: AcceptCallAction) {}
    override suspend fun decline(callId: String) {}
    override suspend fun end(callId: String) {}
    override suspend fun endTyped(callId: String, action: EndCallAction) {}
    override suspend fun hold(callId: String) {}
    override suspend fun unhold(callId: String) {}
    override suspend fun initiate(action: PhoneInitiateAction) {}
    override suspend fun swap() {}
    override suspend fun merge() {}
    override suspend fun mute(muted: Boolean) {}
    override suspend fun dtmf(callId: String?, tone: DtmfTone) {}
    override suspend fun stateGet(): PhoneState = PhoneState(activeCalls = emptyList())
}
