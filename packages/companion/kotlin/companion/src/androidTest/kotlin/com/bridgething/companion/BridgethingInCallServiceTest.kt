package com.bridgething.companion

import android.content.Context
import android.telecom.TelecomManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.bridgething.companion.shell.AndroidPhoneBackend
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.bridgething_companion.InitiateCallType
import uniffi.bridgething_companion.NoHandle
import uniffi.bridgething_companion.PhoneCall
import uniffi.bridgething_companion.PhoneCallDirection
import uniffi.bridgething_companion.PhoneCallStatus
import uniffi.bridgething_companion.PhoneCommand
import uniffi.bridgething_companion.PhoneInitiate
import uniffi.bridgething_companion.PhoneState
import uniffi.bridgething_companion.PhoneStateSink

private class RecordingPhoneStateSink : PhoneStateSink(NoHandle) {
    val results = LinkedBlockingQueue<PhoneState>()

    override fun complete(state: PhoneState) {
        results.add(state)
    }

    override fun fail(reason: String) {
        results.add(PhoneState(activeCalls = emptyList()))
    }
}

@RunWith(AndroidJUnit4::class)
class BridgethingInCallServiceTest {
    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    private lateinit var scope: CoroutineScope
    private lateinit var backend: AndroidPhoneBackend
    private val seen = CopyOnWriteArrayList<PhoneOutEvent>()
    private var collector: Job? = null

    @Before
    fun setUp() {
        val telecom = context.getSystemService(Context.TELECOM_SERVICE) as TelecomManager
        assumeTrue(
            "test APK must be the default dialer (harness: cmd telecom set-default-dialer ${context.packageName})",
            telecom.defaultDialerPackage == context.packageName,
        )
        scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        backend = AndroidPhoneBackend(context)
        collector = scope.launch { PhoneBridgeRegistry.events.collect { seen.add(it) } }
    }

    @After
    fun tearDown() {
        runCatching { backend.command(PhoneCommand.EndTyped("", uniffi.bridgething_companion.EndCallAction.END_ALL)) }
        collector?.cancel()
        if (::scope.isInitialized) scope.cancel()
    }

    @Test
    fun outgoingCallObservedAndControlled() = runBlocking {
        backend.command(
            PhoneCommand.Initiate(
                PhoneInitiate(
                    kind = InitiateCallType.DESTINATION,
                    destinationId = NUMBER,
                    service = null,
                    addressBookId = null,
                ),
            ),
        )

        val call = await("an outgoing call to appear") { activeCalls().firstOrNull() }
        assertNotNull("InCallService should observe the placed call", call)
        assertEquals(PhoneCallDirection.OUTGOING, call!!.direction)
        assertTrue("a CallStarted event should fire", seen.any { it is PhoneOutEvent.CallStarted })

        backend.command(PhoneCommand.End(call.callId))
        await("the call to disconnect") { if (activeCalls().isEmpty()) Unit else null }
        assertTrue("a CallEnded event should fire", seen.any { it is PhoneOutEvent.CallEnded })
    }

    @Test
    fun incomingCallAnsweredAndEnded() = runBlocking {
        val token = InstrumentationRegistry.getArguments().getString("consoleToken")
        assumeTrue("pass -e consoleToken <token> to drive a real modem call", token != null)
        emulatorConsole(token!!, "gsm call $NUMBER")

        val ringing = await("an incoming call to ring") {
            activeCalls().firstOrNull { it.status == PhoneCallStatus.RINGING }
        }
        assertNotNull(ringing)
        assertEquals(PhoneCallDirection.INCOMING, ringing!!.direction)

        backend.command(PhoneCommand.Answer(ringing.callId))
        val active = await("the answered call to go active") {
            activeCalls().firstOrNull { it.status == PhoneCallStatus.ACTIVE }
        }
        assertNotNull("answered call should become active", active)

        backend.command(PhoneCommand.End(active!!.callId))
        await("the call to disconnect") { if (activeCalls().isEmpty()) Unit else null }
    }

    private fun activeCalls(): List<PhoneCall> {
        val sink = RecordingPhoneStateSink()
        backend.stateGet(sink)
        return sink.results.poll(10, TimeUnit.SECONDS)?.activeCalls ?: emptyList()
    }

    private suspend fun <T : Any> await(what: String, timeoutMs: Long = 15_000, probe: suspend () -> T?): T {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            probe()?.let { return it }
            delay(150)
        }
        throw AssertionError("timed out waiting for $what")
    }

    private fun emulatorConsole(token: String, command: String) {
        Socket().use { sock ->
            sock.connect(InetSocketAddress("10.0.2.2", 5554), 5000)
            val out = sock.getOutputStream()
            val reader = BufferedReader(InputStreamReader(sock.getInputStream()))
            fun send(line: String) { out.write((line + "\r\n").toByteArray()); out.flush() }
            fun drain() { while (reader.ready()) reader.readLine() }
            Thread.sleep(300); drain()
            send("auth $token"); Thread.sleep(300); drain()
            send(command); Thread.sleep(300); drain()
        }
    }

    private companion object {
        const val NUMBER = "15555210004"
    }
}
