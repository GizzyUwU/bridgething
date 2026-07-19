package com.bridgething.companion

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.telephony.TelephonyManager
import android.util.Log
import com.bridgething.schema.CallEndReason
import com.bridgething.schema.CommunicationsState
import com.bridgething.schema.PhoneCall
import com.bridgething.schema.PhoneCallDirection
import com.bridgething.schema.PhoneCallEnded
import com.bridgething.schema.PhoneCallService
import com.bridgething.schema.PhoneCallStatus
import com.bridgething.schema.PhoneState
import java.util.UUID

public object PhoneStateTracker {
    private data class Tracked(
        val id: String,
        val remoteId: String,
        val direction: PhoneCallDirection,
        val status: PhoneCallStatus,
        val connectedAtUnixS: Long?,
        val answered: Boolean,
    )

    @Volatile
    private var current: Tracked? = null

    public fun currentState(): PhoneState =
        PhoneState(activeCalls = current?.let { listOf(toWire(it)) } ?: emptyList())

    internal fun handle(state: String, number: String) {
        if (PhoneBridgeRegistry.service != null) return

        when (state) {
            TelephonyManager.EXTRA_STATE_RINGING -> {
                if (current == null) {
                    val call = Tracked(
                        id = UUID.randomUUID().toString(),
                        remoteId = number,
                        direction = PhoneCallDirection.Incoming,
                        status = PhoneCallStatus.Ringing,
                        connectedAtUnixS = null,
                        answered = false,
                    )
                    current = call
                    Log.i(TAG, "incoming ringing ${call.id} (num=${number.isNotEmpty()})")
                    emit(PhoneOutEvent.CallStarted(toWire(call)))
                } else if (current?.remoteId.isNullOrEmpty() && number.isNotEmpty()) {
                    current = current?.copy(remoteId = number)
                    current?.let { emit(PhoneOutEvent.CallUpdated(toWire(it))) }
                }
            }

            TelephonyManager.EXTRA_STATE_OFFHOOK -> {
                val existing = current
                if (existing == null) {
                    val call = Tracked(
                        id = UUID.randomUUID().toString(),
                        remoteId = "",
                        direction = PhoneCallDirection.Outgoing,
                        status = PhoneCallStatus.Active,
                        connectedAtUnixS = nowUnixS(),
                        answered = true,
                    )
                    current = call
                    Log.i(TAG, "outgoing active ${call.id}")
                    emit(PhoneOutEvent.CallStarted(toWire(call)))
                } else if (existing.status == PhoneCallStatus.Ringing) {
                    val answered = existing.copy(
                        status = PhoneCallStatus.Active,
                        connectedAtUnixS = nowUnixS(),
                        answered = true,
                    )
                    current = answered
                    Log.i(TAG, "call answered ${answered.id}")
                    emit(PhoneOutEvent.CallUpdated(toWire(answered)))
                }
            }

            TelephonyManager.EXTRA_STATE_IDLE -> {
                val ended = current ?: return
                current = null
                Log.i(TAG, "call ended ${ended.id}")
                val reason = if (!ended.answered && ended.direction == PhoneCallDirection.Incoming) {
                    CallEndReason.Missed
                } else {
                    CallEndReason.Remote
                }
                emit(PhoneOutEvent.CallEnded(PhoneCallEnded(callId = ended.id, reason = reason)))
            }
        }
        emitSnapshots()
    }

    private fun toWire(c: Tracked): PhoneCall = PhoneCall(
        callId = c.id,
        remoteId = c.remoteId,
        displayName = "",
        status = c.status,
        direction = c.direction,
        startedAtUnixS = c.connectedAtUnixS?.coerceIn(0L, UInt.MAX_VALUE.toLong())?.toUInt(),
        label = null,
        addressBookId = null,
        service = PhoneCallService.Telephony,
        isConferenced = null,
        conferenceGroup = null,
    )

    private fun communications(): CommunicationsState = CommunicationsState(
        muteStatus = null,
        currentCallCount = (if (current != null) 1 else 0).toUByte(),
        telephonyEnabled = true,
        initiateCallAvailable = true,
        holdAvailable = false,
        swapAvailable = false,
        mergeAvailable = false,
    )

    private fun emitSnapshots() {
        emit(PhoneOutEvent.Snapshot(currentState()))
        emit(PhoneOutEvent.Communications(communications()))
    }

    private fun emit(event: PhoneOutEvent) {
        PhoneBridgeRegistry.events.tryEmit(event)
    }

    private fun nowUnixS(): Long = System.currentTimeMillis() / 1000L

    private const val TAG = "bridgething.phone"
}

public class PhoneStateReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context?, intent: Intent?) {
        if (intent?.action != TelephonyManager.ACTION_PHONE_STATE_CHANGED) return
        val state = intent.getStringExtra(TelephonyManager.EXTRA_STATE) ?: return
        @Suppress("DEPRECATION")
        val number = intent.getStringExtra(TelephonyManager.EXTRA_INCOMING_NUMBER).orEmpty()
        PhoneStateTracker.handle(state, number)
    }
}
