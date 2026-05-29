package dev.bridgething.companion

import android.os.Build
import android.telecom.Call
import android.telecom.CallAudioState
import android.telecom.DisconnectCause
import android.telecom.InCallService
import android.telecom.VideoProfile
import android.util.Log
import dev.bridgething.schema.AcceptCallAction
import dev.bridgething.schema.CallEndReason
import dev.bridgething.schema.CallEndReasonFailedInner
import dev.bridgething.schema.CommunicationsState
import dev.bridgething.schema.PhoneCall
import dev.bridgething.schema.PhoneCallDirection
import dev.bridgething.schema.PhoneCallEnded
import dev.bridgething.schema.PhoneCallService
import dev.bridgething.schema.PhoneCallStatus
import dev.bridgething.schema.PhoneState
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/**
 * Android binds this only while the app holds the default-dialer role, which is why full
 * call-control (hold/swap/merge/dtmf/mute) requires that role. Each [Call] gets a
 * companion-stable UUID minted on add and dropped on remove; wire surfaces target calls by that id.
 */
public class BridgethingInCallService : InCallService() {
    private val idForCall = ConcurrentHashMap<Call, String>()
    private val callForId = ConcurrentHashMap<String, Call>()

    private val callback = object : Call.Callback() {
        override fun onStateChanged(call: Call, state: Int) {
            val id = idForCall[call] ?: return
            emit(PhoneOutEvent.CallUpdated(toWire(call, id)))
            emitSnapshots()
        }

        override fun onDetailsChanged(call: Call, details: Call.Details) {
            val id = idForCall[call] ?: return
            emit(PhoneOutEvent.CallUpdated(toWire(call, id)))
        }
    }

    override fun onCallAdded(call: Call) {
        val id = UUID.randomUUID().toString()
        idForCall[call] = id
        callForId[id] = call
        call.registerCallback(callback)
        PhoneBridgeRegistry.service = this
        Log.i(TAG, "call added $id state=${stateOf(call)}")
        emit(PhoneOutEvent.CallStarted(toWire(call, id)))
        emitSnapshots()
    }

    override fun onCallRemoved(call: Call) {
        val id = idForCall.remove(call) ?: return
        callForId.remove(id)
        call.unregisterCallback(callback)
        Log.i(TAG, "call removed $id")
        emit(PhoneOutEvent.CallEnded(PhoneCallEnded(callId = id, reason = endReason(call))))
        emitSnapshots()
    }

    // CallAudioState deprecated at API 34 for CallEndpoint, but it is the only mute signal on minSdk 26.
    @Suppress("OVERRIDE_DEPRECATION")
    override fun onCallAudioStateChanged(audioState: CallAudioState?) {
        emit(PhoneOutEvent.Communications(communications()))
    }

    override fun onDestroy() {
        if (PhoneBridgeRegistry.service === this) PhoneBridgeRegistry.service = null
        super.onDestroy()
    }

    // MARK: - control surface (driven by AndroidPhoneBackend)

    public fun answerCall(id: String) {
        callForId[id]?.answer(VideoProfile.STATE_AUDIO_ONLY)
    }

    public fun accept(id: String, action: AcceptCallAction) {
        if (action == AcceptCallAction.EndAndAccept) callForId.values.firstOrNull { stateOf(it) == Call.STATE_ACTIVE }?.disconnect()
        callForId[id]?.answer(VideoProfile.STATE_AUDIO_ONLY)
    }

    public fun rejectCall(id: String) {
        val call = callForId[id] ?: return
        if (stateOf(call) == Call.STATE_RINGING) call.reject(false, null) else call.disconnect()
    }

    public fun endCall(id: String) {
        callForId[id]?.disconnect()
    }

    public fun endAll() {
        callForId.values.forEach { it.disconnect() }
    }

    public fun holdCall(id: String) {
        callForId[id]?.hold()
    }

    public fun unholdCall(id: String) {
        callForId[id]?.unhold()
    }

    // telecom has no swap verb; holding the active call causes the framework to foreground the held one
    public fun swap() {
        callForId.values.firstOrNull { stateOf(it) == Call.STATE_ACTIVE }?.hold()
    }

    public fun merge() {
        val host = callForId.values.firstOrNull { stateOf(it) == Call.STATE_ACTIVE } ?: return
        host.conferenceableCalls.firstOrNull()?.let { host.conference(it) }
    }

    public fun mute(muted: Boolean) {
        setMuted(muted)
    }

    public fun playDtmf(id: String?, tone: Char) {
        val call = id?.let { callForId[it] } ?: callForId.values.firstOrNull { stateOf(it) == Call.STATE_ACTIVE } ?: return
        call.playDtmfTone(tone)
        call.stopDtmfTone()
    }

    public fun currentState(): PhoneState = PhoneState(activeCalls = callForId.map { (id, call) -> toWire(call, id) })

    // MARK: - mapping

    private fun toWire(call: Call, id: String): PhoneCall {
        val details = call.details
        val handle = details?.handle?.schemeSpecificPart ?: ""
        val name = details?.callerDisplayName?.takeIf { it.isNotEmpty() } ?: ""
        val connectTime = details?.connectTimeMillis ?: 0L
        return PhoneCall(
            callId = id,
            remoteId = handle,
            displayName = name,
            status = mapStatus(stateOf(call)),
            direction = mapDirection(call),
            startedAtUnixS = (connectTime / 1000L).takeIf { connectTime > 0L }?.coerceIn(0L, UInt.MAX_VALUE.toLong())?.toUInt(),
            label = null,
            addressBookId = null,
            service = PhoneCallService.Telephony,
            isConferenced = (call.parent != null).takeIf { it },
            conferenceGroup = null,
        )
    }

    // Details.getState() only exists from API 31; this suppressed path keeps call sites clean on minSdk 26.
    @Suppress("DEPRECATION")
    private fun stateOf(call: Call): Int = call.state

    private fun mapStatus(state: Int): PhoneCallStatus = when (state) {
        Call.STATE_DIALING -> PhoneCallStatus.Sending
        Call.STATE_RINGING -> PhoneCallStatus.Ringing
        Call.STATE_HOLDING -> PhoneCallStatus.Held
        Call.STATE_ACTIVE -> PhoneCallStatus.Active
        Call.STATE_DISCONNECTING -> PhoneCallStatus.Disconnecting
        Call.STATE_DISCONNECTED -> PhoneCallStatus.Disconnected
        else -> PhoneCallStatus.Connecting
    }

    private fun mapDirection(call: Call): PhoneCallDirection {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            return when (call.details?.callDirection) {
                Call.Details.DIRECTION_OUTGOING -> PhoneCallDirection.Outgoing
                else -> PhoneCallDirection.Incoming
            }
        }
        return if (stateOf(call) == Call.STATE_DIALING || stateOf(call) == Call.STATE_CONNECTING) {
            PhoneCallDirection.Outgoing
        } else {
            PhoneCallDirection.Incoming
        }
    }

    private fun endReason(call: Call): CallEndReason = when (call.details?.disconnectCause?.code) {
        DisconnectCause.LOCAL -> CallEndReason.Local
        DisconnectCause.REMOTE -> CallEndReason.Remote
        DisconnectCause.MISSED -> CallEndReason.Missed
        DisconnectCause.REJECTED -> CallEndReason.Declined
        DisconnectCause.CANCELED -> CallEndReason.Local
        null -> CallEndReason.Remote
        else -> CallEndReason.Failed(CallEndReasonFailedInner(reason = call.details?.disconnectCause?.toString() ?: "unknown"))
    }

    @Suppress("DEPRECATION")
    private fun communications(): CommunicationsState {
        val calls = callForId.values
        fun anyCan(capability: Int) = calls.any { it.details?.can(capability) == true }
        return CommunicationsState(
            muteStatus = callAudioState?.isMuted,
            currentCallCount = calls.size.coerceIn(0, 255).toUByte(),
            telephonyEnabled = true,
            initiateCallAvailable = true,
            holdAvailable = anyCan(Call.Details.CAPABILITY_HOLD),
            swapAvailable = anyCan(Call.Details.CAPABILITY_SWAP_CONFERENCE),
            mergeAvailable = anyCan(Call.Details.CAPABILITY_MERGE_CONFERENCE),
        )
    }

    private fun emitSnapshots() {
        emit(PhoneOutEvent.Snapshot(currentState()))
        emit(PhoneOutEvent.Communications(communications()))
    }

    private fun emit(event: PhoneOutEvent) {
        PhoneBridgeRegistry.events.tryEmit(event)
    }

    private companion object {
        const val TAG = "bridgething.phone"
    }
}
