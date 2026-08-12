package com.bridgething.companion

import android.os.Build
import android.telecom.Call
import android.telecom.CallAudioState
import android.telecom.DisconnectCause
import android.telecom.InCallService
import android.telecom.VideoProfile
import android.util.Log
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import uniffi.bridgething_companion.AcceptCallAction
import uniffi.bridgething_companion.CallEndReason
import uniffi.bridgething_companion.CommunicationsState
import uniffi.bridgething_companion.PhoneCall
import uniffi.bridgething_companion.PhoneCallDirection
import uniffi.bridgething_companion.PhoneCallEnded
import uniffi.bridgething_companion.PhoneCallService
import uniffi.bridgething_companion.PhoneCallStatus
import uniffi.bridgething_companion.PhoneState

public class BridgethingInCallService : InCallService() {
    private val idForCall = ConcurrentHashMap<Call, String>()
    private val callForId = ConcurrentHashMap<String, Call>()

    private val callback = object : Call.Callback() {
        override fun onStateChanged(call: Call, state: Int) {
            val id = idForCall[call] ?: return
            emit(PhoneOutEvent.CallUpdated(toCore(call, id)))
            emitSnapshots()
        }

        override fun onDetailsChanged(call: Call, details: Call.Details) {
            val id = idForCall[call] ?: return
            emit(PhoneOutEvent.CallUpdated(toCore(call, id)))
        }
    }

    override fun onCallAdded(call: Call) {
        val id = UUID.randomUUID().toString()
        idForCall[call] = id
        callForId[id] = call
        call.registerCallback(callback)
        PhoneBridgeRegistry.service = this
        Log.i(TAG, "call added $id state=${stateOf(call)}")
        emit(PhoneOutEvent.CallStarted(toCore(call, id)))
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

    @Suppress("OVERRIDE_DEPRECATION")
    override fun onCallAudioStateChanged(audioState: CallAudioState?) {
        emit(PhoneOutEvent.Communications(communications()))
    }

    override fun onDestroy() {
        if (PhoneBridgeRegistry.service === this) PhoneBridgeRegistry.service = null
        super.onDestroy()
    }

    // control surface

    public fun answerCall(id: String) {
        callForId[id]?.answer(VideoProfile.STATE_AUDIO_ONLY)
    }

    public fun accept(id: String, action: AcceptCallAction) {
        if (action == AcceptCallAction.END_AND_ACCEPT) callForId.values.firstOrNull { stateOf(it) == Call.STATE_ACTIVE }?.disconnect()
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

    public fun currentState(): PhoneState = PhoneState(activeCalls = callForId.map { (id, call) -> toCore(call, id) })

    // mapping

    private fun toCore(call: Call, id: String): PhoneCall {
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
            service = PhoneCallService.TELEPHONY,
            isConferenced = (call.parent != null).takeIf { it },
            conferenceGroup = null,
        )
    }

    @Suppress("DEPRECATION")
    private fun stateOf(call: Call): Int = call.state

    private fun mapStatus(state: Int): PhoneCallStatus = when (state) {
        Call.STATE_DIALING -> PhoneCallStatus.SENDING
        Call.STATE_RINGING -> PhoneCallStatus.RINGING
        Call.STATE_HOLDING -> PhoneCallStatus.HELD
        Call.STATE_ACTIVE -> PhoneCallStatus.ACTIVE
        Call.STATE_DISCONNECTING -> PhoneCallStatus.DISCONNECTING
        Call.STATE_DISCONNECTED -> PhoneCallStatus.DISCONNECTED
        else -> PhoneCallStatus.CONNECTING
    }

    private fun mapDirection(call: Call): PhoneCallDirection {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            return when (call.details?.callDirection) {
                Call.Details.DIRECTION_OUTGOING -> PhoneCallDirection.OUTGOING
                else -> PhoneCallDirection.INCOMING
            }
        }
        return if (stateOf(call) == Call.STATE_DIALING || stateOf(call) == Call.STATE_CONNECTING) {
            PhoneCallDirection.OUTGOING
        } else {
            PhoneCallDirection.INCOMING
        }
    }

    private fun endReason(call: Call): CallEndReason = when (call.details?.disconnectCause?.code) {
        DisconnectCause.LOCAL -> CallEndReason.Local
        DisconnectCause.REMOTE -> CallEndReason.Remote
        DisconnectCause.MISSED -> CallEndReason.Missed
        DisconnectCause.REJECTED -> CallEndReason.Declined
        DisconnectCause.CANCELED -> CallEndReason.Local
        null -> CallEndReason.Remote
        else -> CallEndReason.Failed(call.details?.disconnectCause?.toString() ?: "unknown")
    }

    @Suppress("DEPRECATION")
    private fun communications(): CommunicationsState {
        val calls = callForId.values
        fun anyCan(capability: Int) = calls.any { it.details?.can(capability) == true }
        return telephonyCommunications(
            currentCallCount = calls.size.coerceIn(0, 255).toUByte(),
            muteStatus = callAudioState?.isMuted,
            swapAvailable = anyCan(Call.Details.CAPABILITY_SWAP_CONFERENCE),
            mergeAvailable = anyCan(Call.Details.CAPABILITY_MERGE_CONFERENCE),
            holdAvailable = anyCan(Call.Details.CAPABILITY_HOLD),
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
