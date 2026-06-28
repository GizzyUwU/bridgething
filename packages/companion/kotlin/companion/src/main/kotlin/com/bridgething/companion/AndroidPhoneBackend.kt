package com.bridgething.companion

import android.content.Context
import android.net.Uri
import android.os.Bundle
import android.provider.CallLog
import android.telecom.TelecomManager
import com.bridgething.schema.AcceptCallAction
import com.bridgething.schema.DtmfTone
import com.bridgething.schema.EndCallAction
import com.bridgething.schema.InitiateCallType
import com.bridgething.schema.PhoneInitiateAction
import com.bridgething.schema.PhoneState
import kotlinx.coroutines.flow.Flow

public class AndroidPhoneBackend(
    private val context: Context,
) : PhoneBackend {
    public override val events: Flow<PhoneOutEvent> = PhoneBridgeRegistry.events

    private fun service(): BridgethingInCallService? = PhoneBridgeRegistry.service

    public override suspend fun answer(callId: String) {
        service()?.answerCall(callId)
    }

    public override suspend fun accept(callId: String, action: AcceptCallAction) {
        service()?.accept(callId, action)
    }

    public override suspend fun decline(callId: String) {
        service()?.rejectCall(callId)
    }

    public override suspend fun end(callId: String) {
        service()?.endCall(callId)
    }

    public override suspend fun endTyped(callId: String, action: EndCallAction) {
        if (action == EndCallAction.EndAll) service()?.endAll() else service()?.endCall(callId)
    }

    public override suspend fun hold(callId: String) {
        service()?.holdCall(callId)
    }

    public override suspend fun unhold(callId: String) {
        service()?.unholdCall(callId)
    }

    public override suspend fun initiate(action: PhoneInitiateAction) {
        val uri = when (action.kind) {
            InitiateCallType.Destination -> action.destinationId?.let { Uri.fromParts("tel", it, null) }
            InitiateCallType.Voicemail -> Uri.fromParts("voicemail", "", null)
            InitiateCallType.Redial -> lastDialed()?.let { Uri.fromParts("tel", it, null) }
        } ?: return
        val telecom = context.getSystemService(Context.TELECOM_SERVICE) as? TelecomManager ?: return
        // CALL_PHONE may be ungranted; swallow rather than crash the dispatcher.
        runCatching { telecom.placeCall(uri, Bundle()) }
    }

    public override suspend fun swap() {
        service()?.swap()
    }

    public override suspend fun merge() {
        service()?.merge()
    }

    public override suspend fun mute(muted: Boolean) {
        service()?.mute(muted)
    }

    public override suspend fun dtmf(callId: String?, tone: DtmfTone) {
        service()?.playDtmf(callId, tone.toChar())
    }

    public override suspend fun stateGet(): PhoneState = service()?.currentState() ?: PhoneState(activeCalls = emptyList())

    private fun lastDialed(): String? = runCatching {
        context.contentResolver.query(
            CallLog.Calls.CONTENT_URI,
            arrayOf(CallLog.Calls.NUMBER),
            "${CallLog.Calls.TYPE} = ?",
            arrayOf(CallLog.Calls.OUTGOING_TYPE.toString()),
            "${CallLog.Calls.DATE} DESC",
        )?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
    }.getOrNull()
}

private fun DtmfTone.toChar(): Char = when (this) {
    DtmfTone.D0 -> '0'
    DtmfTone.D1 -> '1'
    DtmfTone.D2 -> '2'
    DtmfTone.D3 -> '3'
    DtmfTone.D4 -> '4'
    DtmfTone.D5 -> '5'
    DtmfTone.D6 -> '6'
    DtmfTone.D7 -> '7'
    DtmfTone.D8 -> '8'
    DtmfTone.D9 -> '9'
    DtmfTone.Star -> '*'
    DtmfTone.Hash -> '#'
}
