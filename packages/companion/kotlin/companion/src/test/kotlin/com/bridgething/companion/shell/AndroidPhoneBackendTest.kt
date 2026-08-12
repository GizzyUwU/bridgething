package com.bridgething.companion.shell

import android.content.Context
import com.bridgething.companion.BridgethingInCallService
import com.bridgething.companion.PhoneBridgeRegistry
import com.bridgething.companion.PhoneOutEvent
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.AcceptCallAction
import uniffi.bridgething_companion.CallEndReason
import uniffi.bridgething_companion.NoHandle
import uniffi.bridgething_companion.PhoneCall
import uniffi.bridgething_companion.PhoneCallDirection
import uniffi.bridgething_companion.PhoneCallEnded
import uniffi.bridgething_companion.PhoneCallStatus
import uniffi.bridgething_companion.PhoneCommand
import uniffi.bridgething_companion.PhoneInbox
import uniffi.bridgething_companion.PhoneState
import uniffi.bridgething_companion.PhoneStateSink

private class RecordingPhoneInbox : PhoneInbox(NoHandle) {
    val events = LinkedBlockingQueue<Any>()

    override fun onCallStarted(call: PhoneCall) {
        events.add("started" to call)
    }

    override fun onCallUpdated(call: PhoneCall) {
        events.add("updated" to call)
    }

    override fun onCallEnded(ended: PhoneCallEnded) {
        events.add("ended" to ended)
    }

    override fun onState(state: PhoneState) {
        events.add("state" to state)
    }

    override fun onCommunications(state: uniffi.bridgething_companion.CommunicationsState) {
        events.add("communications" to state)
    }
}

private class RecordingStateSink : PhoneStateSink(NoHandle) {
    val outcomes = LinkedBlockingQueue<Any>()

    override fun complete(state: PhoneState) {
        outcomes.add(state)
    }

    override fun fail(reason: String) {
        outcomes.add(Exception(reason))
    }
}

class AndroidPhoneBackendTest {
    private val timeoutMs = 5_000L

    @AfterEach
    fun clearRegistry() {
        PhoneBridgeRegistry.service = null
    }

    private fun call(id: String) = PhoneCall(
        callId = id,
        remoteId = "+15551234567",
        displayName = "Test Caller",
        status = PhoneCallStatus.RINGING,
        direction = PhoneCallDirection.INCOMING,
        startedAtUnixS = null,
        label = null,
        addressBookId = null,
        service = null,
        isConferenced = null,
        conferenceGroup = null,
    )

    private fun backend(): AndroidPhoneBackend = AndroidPhoneBackend(mockk<Context>(relaxed = true))

    @Test
    fun registryEventsReachTheCoreInbox() {
        val backend = backend()
        val inbox = RecordingPhoneInbox()
        backend.start(inbox)
        try {
            runBlocking { withTimeout(timeoutMs) { PhoneBridgeRegistry.events.subscriptionCount.first { it > 0 } } }

            PhoneBridgeRegistry.events.tryEmit(PhoneOutEvent.CallStarted(call("c1")))
            assertEquals("started" to call("c1"), inbox.events.poll(timeoutMs, TimeUnit.MILLISECONDS))

            PhoneBridgeRegistry.events.tryEmit(
                PhoneOutEvent.CallEnded(PhoneCallEnded(callId = "c1", reason = CallEndReason.Remote)),
            )
            assertEquals(
                "ended" to PhoneCallEnded(callId = "c1", reason = CallEndReason.Remote),
                inbox.events.poll(timeoutMs, TimeUnit.MILLISECONDS),
            )
        } finally {
            backend.stop()
        }
    }

    @Test
    fun commandsReachTheBoundInCallService() {
        val svc = mockk<BridgethingInCallService>(relaxed = true)
        PhoneBridgeRegistry.service = svc
        val backend = backend()

        backend.command(PhoneCommand.Answer(callId = "c1"))
        verify(timeout = timeoutMs) { svc.answerCall("c1") }

        backend.command(PhoneCommand.Accept(callId = "c2", action = AcceptCallAction.END_AND_ACCEPT))
        verify(timeout = timeoutMs) { svc.accept("c2", AcceptCallAction.END_AND_ACCEPT) }

        backend.command(PhoneCommand.Dtmf(callId = "c1", tone = uniffi.bridgething_companion.DtmfTone.STAR))
        verify(timeout = timeoutMs) { svc.playDtmf("c1", '*') }

        backend.command(PhoneCommand.Mute(muted = true))
        verify(timeout = timeoutMs) { svc.mute(true) }
    }

    @Test
    fun stateGetAnswersFromTheServiceWhenBound() {
        val svc = mockk<BridgethingInCallService>(relaxed = true)
        every { svc.currentState() } returns PhoneState(activeCalls = listOf(call("c9")))
        PhoneBridgeRegistry.service = svc
        val backend = backend()

        val sink = RecordingStateSink()
        backend.stateGet(sink)
        assertEquals(
            PhoneState(activeCalls = listOf(call("c9"))),
            sink.outcomes.poll(timeoutMs, TimeUnit.MILLISECONDS),
        )
    }
}
