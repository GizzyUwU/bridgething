package com.bridgething.companion.shell

import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.CallLog
import android.telecom.TelecomManager
import android.util.Log
import com.bridgething.companion.BridgethingInCallService
import com.bridgething.companion.PhoneBridgeRegistry
import com.bridgething.companion.PhoneOutEvent
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import uniffi.bridgething_companion.AcceptCallAction
import uniffi.bridgething_companion.DtmfTone
import uniffi.bridgething_companion.EndCallAction
import uniffi.bridgething_companion.InitiateCallType
import uniffi.bridgething_companion.PhoneBackend
import uniffi.bridgething_companion.PhoneCommand
import uniffi.bridgething_companion.PhoneInbox
import uniffi.bridgething_companion.PhoneInitiate
import uniffi.bridgething_companion.PhoneStateSink

public class AndroidPhoneBackend(
    private val context: Context,
) : PhoneBackend {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default + CoroutineName("bridgething-phone"))

    @Volatile
    private var relay: Job? = null

    private val telecom: TelecomManager?
        get() = context.getSystemService(Context.TELECOM_SERVICE) as? TelecomManager

    private fun service(): BridgethingInCallService? = PhoneBridgeRegistry.service

    override fun start(inbox: PhoneInbox) {
        relay?.cancel()
        relay = scope.launch {
            try {
                PhoneBridgeRegistry.events.collect { event ->
                    when (event) {
                        is PhoneOutEvent.CallStarted -> inbox.onCallStarted(event.call)
                        is PhoneOutEvent.CallUpdated -> inbox.onCallUpdated(event.call)
                        is PhoneOutEvent.CallEnded -> inbox.onCallEnded(event.ended)
                        is PhoneOutEvent.Snapshot -> inbox.onState(event.state)
                        is PhoneOutEvent.Communications -> inbox.onCommunications(event.state)
                    }
                }
            } finally {
                inbox.close()
            }
        }
    }

    override fun stop() {
        relay?.cancel()
        relay = null
    }

    override fun command(cmd: PhoneCommand) {
        scope.launch {
            when (cmd) {
                is PhoneCommand.Answer -> service()?.answerCall(cmd.callId) ?: telecomAccept()
                is PhoneCommand.Accept -> {
                    val svc = service()
                    if (svc != null) {
                        svc.accept(cmd.callId, cmd.action)
                    } else {
                        if (cmd.action == AcceptCallAction.END_AND_ACCEPT) telecomEnd()
                        telecomAccept()
                    }
                }
                is PhoneCommand.Decline -> service()?.rejectCall(cmd.callId) ?: telecomEnd()
                is PhoneCommand.End -> service()?.endCall(cmd.callId) ?: telecomEnd()
                is PhoneCommand.EndTyped -> {
                    val svc = service()
                    if (svc != null) {
                        if (cmd.action == EndCallAction.END_ALL) svc.endAll() else svc.endCall(cmd.callId)
                    } else {
                        telecomEnd()
                    }
                }
                is PhoneCommand.Hold -> service()?.holdCall(cmd.callId)
                is PhoneCommand.Unhold -> service()?.unholdCall(cmd.callId)
                is PhoneCommand.Swap -> service()?.swap()
                is PhoneCommand.Merge -> service()?.merge()
                is PhoneCommand.Mute -> service()?.mute(cmd.muted)
                is PhoneCommand.Dtmf -> service()?.playDtmf(cmd.callId, toDtmfChar(cmd.tone))
                is PhoneCommand.Initiate -> initiate(cmd.action)
            }
        }
    }

    override fun stateGet(sink: PhoneStateSink) {
        scope.launch {
            sink.use { it.complete(service()?.currentState() ?: com.bridgething.companion.PhoneStateTracker.currentState()) }
        }
    }

    private fun initiate(action: PhoneInitiate) {
        val uri = when (action.kind) {
            InitiateCallType.DESTINATION -> action.destinationId?.let { Uri.fromParts("tel", it, null) }
            InitiateCallType.VOICEMAIL -> Uri.fromParts("voicemail", "", null)
            InitiateCallType.REDIAL -> lastDialed()?.let { Uri.fromParts("tel", it, null) }
        } ?: return
        val tm = telecom ?: return
        runCatching { tm.placeCall(uri, Bundle()) }
    }

    @SuppressLint("MissingPermission")
    private fun telecomAccept() {
        if (!granted(android.Manifest.permission.ANSWER_PHONE_CALLS)) return
        runCatching { telecom?.acceptRingingCall() }
            .onFailure { Log.w(TAG, "acceptRingingCall failed: ${it.message}") }
    }

    @SuppressLint("MissingPermission")
    private fun telecomEnd() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return
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

private fun toDtmfChar(tone: DtmfTone): Char = when (tone) {
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
    DtmfTone.STAR -> '*'
    DtmfTone.HASH -> '#'
}
