package dev.bridgething.companion

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.schema.AcceptCallAction
import dev.bridgething.schema.CommunicationsState
import dev.bridgething.schema.DtmfTone
import dev.bridgething.schema.EndCallAction
import dev.bridgething.schema.PhoneCall
import dev.bridgething.schema.PhoneCallEnded
import dev.bridgething.schema.PhoneInitiateAction
import dev.bridgething.schema.PhoneState
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import java.util.UUID

/**
 * Geo backend seam. [GeoController] is the real (FusedLocation / LocationManager)
 * impl; tests inject a no-op so the companion boots without touching location services.
 */
public interface GeoSource {
    public suspend fun start(gateway: BridgethingGateway)
    public suspend fun stop()
}

/**
 * Volume backend seam. [VolumeMonitor] is the real [android.media.AudioManager]
 * impl (which touches Android at construction); tests inject a no-op so the
 * companion can run on a plain JVM without Robolectric.
 */
public interface VolumeSource {
    public fun interface Callback {
        public fun onVolumeChanged(level: Float, muted: Boolean)
    }

    public fun start(callback: Callback)
    public fun stop()
    public fun snapshot(): Pair<Float, Boolean>
}

/**
 * Audio backend seam the [AudioDispatcher] drives. [AndroidAudioBackend] is the
 * real impl (TextToSpeech + AudioManager); tests inject a fake so dispatch
 * routing runs on a plain JVM. Android implements every verb (no native audio sidechannel like iOS AMS).
 */
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

/**
 * Notification-action backend seam. iOS handles notification actions over ANCS, so the
 * `invokePositive` / `invokeNegative` path is Android-only; the SDK default is a no-op and the
 * host app injects [AndroidNotificationActionBackend] wired to its NotificationListenerService.
 */
public interface NotificationActionBackend {
    public suspend fun invokePositive(id: String)
    public suspend fun invokeNegative(id: String)
}

/** Default backend: drops invokes (iOS uses ANCS; an Android host injects a real one). */
public object NoOpNotificationActionBackend : NotificationActionBackend {
    override suspend fun invokePositive(id: String) {}
    override suspend fun invokeNegative(id: String) {}
}

/** outbound telephony events the [PhoneBackend] observes and the [PhoneDispatcher] relays to `gateway.phone.*`. */
public sealed interface PhoneOutEvent {
    public data class CallStarted(val call: PhoneCall) : PhoneOutEvent
    public data class CallUpdated(val call: PhoneCall) : PhoneOutEvent
    public data class CallEnded(val ended: PhoneCallEnded) : PhoneOutEvent
    public data class Snapshot(val state: PhoneState) : PhoneOutEvent
    public data class Communications(val state: CommunicationsState) : PhoneOutEvent
}

/**
 * Telephony backend seam the [PhoneDispatcher] drives. iOS handles telephony over iAP2 straight to the daemon,
 * so the gateway phone surface is Android-only; the SDK default is a no-op and an Android host injects a real
 * InCallService-backed impl. Full call-control requires `InCallService`, which Android binds only when the app
 * holds the default-dialer role.
 */
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

/** Default backend: no telephony (iOS uses iAP2; an Android host injects a real one). */
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
