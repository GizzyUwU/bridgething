package com.bridgething.companion

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.CallLog
import android.telecom.TelecomManager
import android.util.Log
import com.bridgething.schema.AcceptCallAction
import com.bridgething.schema.DtmfTone
import com.bridgething.schema.EndCallAction
import com.bridgething.schema.InitiateCallType
import com.bridgething.schema.PhoneInitiateAction
import com.bridgething.schema.PhoneState
import kotlinx.coroutines.flow.Flow

/**
 * Composite telephony backend. When the app holds the default-dialer role the OS binds the
 * [BridgethingInCallService] and it is authoritative (full multi-call control). Otherwise the
 * light path drives it: [PhoneStateWatcher] synthesizes call events from the PHONE_STATE
 * broadcast, and answer/decline/end go through [TelecomManager] under `ANSWER_PHONE_CALLS` -
 * no dialer role, no call-screening role. Hold/swap/merge/mute/DTMF stay InCallService-only.
 */
public class AndroidPhoneBackend(
    private val context: Context,
) : PhoneBackend {
    private val telecom: TelecomManager?
        get() = context.getSystemService(Context.TELECOM_SERVICE) as? TelecomManager

    // Runs whenever the app is NOT the default dialer; emits into PhoneBridgeRegistry.events,
    // gated so it goes silent the moment the InCallService binds.
    private val watcher = PhoneStateWatcher(context).also { it.start() }

    public override val events: Flow<PhoneOutEvent> = PhoneBridgeRegistry.events

    private fun service(): BridgethingInCallService? = PhoneBridgeRegistry.service

    public override suspend fun answer(callId: String) {
        service()?.answerCall(callId) ?: telecomAccept()
    }

    public override suspend fun accept(callId: String, action: AcceptCallAction) {
        val svc = service()
        if (svc != null) {
            svc.accept(callId, action)
            return
        }
        if (action == AcceptCallAction.EndAndAccept) telecomEnd()
        telecomAccept()
    }

    public override suspend fun decline(callId: String) {
        service()?.rejectCall(callId) ?: telecomEnd()
    }

    public override suspend fun end(callId: String) {
        service()?.endCall(callId) ?: telecomEnd()
    }

    public override suspend fun endTyped(callId: String, action: EndCallAction) {
        val svc = service()
        if (svc != null) {
            if (action == EndCallAction.EndAll) svc.endAll() else svc.endCall(callId)
            return
        }
        telecomEnd()
    }

    // Multi-call control is InCallService-only (the opt-in dialer role); no-op on the light path.
    public override suspend fun hold(callId: String) { service()?.holdCall(callId) }
    public override suspend fun unhold(callId: String) { service()?.unholdCall(callId) }
    public override suspend fun swap() { service()?.swap() }
    public override suspend fun merge() { service()?.merge() }
    public override suspend fun mute(muted: Boolean) { service()?.mute(muted) }
    public override suspend fun dtmf(callId: String?, tone: DtmfTone) { service()?.playDtmf(callId, tone.toChar()) }

    public override suspend fun initiate(action: PhoneInitiateAction) {
        val uri = when (action.kind) {
            InitiateCallType.Destination -> action.destinationId?.let { Uri.fromParts("tel", it, null) }
            InitiateCallType.Voicemail -> Uri.fromParts("voicemail", "", null)
            InitiateCallType.Redial -> lastDialed()?.let { Uri.fromParts("tel", it, null) }
        } ?: return
        val tm = telecom ?: return
        // CALL_PHONE may be ungranted; swallow rather than crash the dispatcher.
        runCatching { tm.placeCall(uri, Bundle()) }
    }

    public override suspend fun stateGet(): PhoneState =
        service()?.currentState() ?: watcher.currentState()

    // MARK: - dialer-less control via TelecomManager (ANSWER_PHONE_CALLS)

    @SuppressLint("MissingPermission") // gated on the runtime grant below
    private fun telecomAccept() {
        if (!granted(android.Manifest.permission.ANSWER_PHONE_CALLS)) return
        runCatching { telecom?.acceptRingingCall() }
            .onFailure { Log.w(TAG, "acceptRingingCall failed: ${it.message}") }
    }

    @SuppressLint("MissingPermission") // gated on the runtime grant below
    private fun telecomEnd() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return // TelecomManager.endCall() is API 28+
        if (!granted(android.Manifest.permission.ANSWER_PHONE_CALLS)) return
        runCatching { telecom?.endCall() }
            .onFailure { Log.w(TAG, "endCall failed: ${it.message}") }
    }

    private fun granted(perm: String): Boolean =
        context.checkSelfPermission(perm) == PackageManager.PERMISSION_GRANTED

    private fun lastDialed(): String? = runCatching {
        context.contentResolver.query(
            CallLog.Calls.CONTENT_URI,
            arrayOf(CallLog.Calls.NUMBER),
            "${CallLog.Calls.TYPE} = ?",
            arrayOf(CallLog.Calls.OUTGOING_TYPE.toString()),
            "${CallLog.Calls.DATE} DESC",
        )?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
    }.getOrNull()

    private companion object {
        const val TAG = "bridgething.phone"
    }
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
