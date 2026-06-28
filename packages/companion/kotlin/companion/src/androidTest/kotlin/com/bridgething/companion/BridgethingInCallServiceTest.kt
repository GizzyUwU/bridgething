package com.bridgething.companion

import android.content.Context
import android.telecom.TelecomManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.bridgething.schema.InitiateCallType
import com.bridgething.schema.PhoneCallDirection
import com.bridgething.schema.PhoneCallStatus
import com.bridgething.schema.PhoneInitiateAction
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
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Real-tier telephony test: drives [AndroidPhoneBackend] against the live
 * [BridgethingInCallService] on the emulator with nothing mocked. Telecom binds
 * the InCallService only while the test APK is the default dialer, so the
 * harness (notes/companion or the just recipe) sets that + grants CALL_PHONE
 * before `am instrument`; the test bails out cleanly otherwise.
 *
 * `outgoingCall...` places a real call through TelecomManager and exercises the
 * full observe + control loop. `incomingCall...` injects a real modem call via
 * the emulator console (host loopback 10.0.2.2:5554) and is skipped unless the
 * console auth token is passed as an instrumentation arg.
 */
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
        collector = scope.launch { backend.events.collect { seen.add(it) } }
    }

    @After
    fun tearDown() {
        runBlocking { runCatching { backend.endTyped("", com.bridgething.schema.EndCallAction.EndAll) } }
        collector?.cancel()
        if (::scope.isInitialized) scope.cancel()
    }

    @Test
    fun outgoingCallObservedAndControlled() = runBlocking {
        backend.initiate(PhoneInitiateAction(kind = InitiateCallType.Destination, destinationId = NUMBER))

        val call = await("an outgoing call to appear") { backend.stateGet().activeCalls.firstOrNull() }
        assertNotNull("InCallService should observe the placed call", call)
        assertEquals(PhoneCallDirection.Outgoing, call!!.direction)
        assertTrue("a CallStarted event should fire", seen.any { it is PhoneOutEvent.CallStarted })

        backend.end(call.callId)
        await("the call to disconnect") { if (backend.stateGet().activeCalls.isEmpty()) Unit else null }
        assertTrue("a CallEnded event should fire", seen.any { it is PhoneOutEvent.CallEnded })
    }

    @Test
    fun incomingCallAnsweredAndEnded() = runBlocking {
        val token = InstrumentationRegistry.getArguments().getString("consoleToken")
        assumeTrue("pass -e consoleToken <token> to drive a real modem call", token != null)
        emulatorConsole(token!!, "gsm call $NUMBER")

        val ringing = await("an incoming call to ring") {
            backend.stateGet().activeCalls.firstOrNull { it.status == PhoneCallStatus.Ringing }
        }
        assertNotNull(ringing)
        assertEquals(PhoneCallDirection.Incoming, ringing!!.direction)

        backend.answer(ringing.callId)
        val active = await("the answered call to go active") {
            backend.stateGet().activeCalls.firstOrNull { it.status == PhoneCallStatus.Active }
        }
        assertNotNull("answered call should become active", active)

        backend.end(active!!.callId)
        await("the call to disconnect") { if (backend.stateGet().activeCalls.isEmpty()) Unit else null }
    }

    private suspend fun <T : Any> await(what: String, timeoutMs: Long = 15_000, probe: suspend () -> T?): T {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            probe()?.let { return it }
            delay(150)
        }
        throw AssertionError("timed out waiting for $what")
    }

    // Drive the emulator's modem via its console on the host loopback. Auth then
    // run one command; the modem produces a genuine telephony Call.
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
