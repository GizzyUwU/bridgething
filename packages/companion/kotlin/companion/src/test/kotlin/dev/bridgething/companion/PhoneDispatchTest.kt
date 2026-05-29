package dev.bridgething.companion

import dev.bridgething.schema.AcceptCallAction
import dev.bridgething.schema.BridgeToGatewayMsgData
import dev.bridgething.schema.BridgeToGatewayPhoneMsg
import dev.bridgething.schema.CommunicationsState
import dev.bridgething.schema.DtmfTone
import dev.bridgething.schema.EndCallAction
import dev.bridgething.schema.GatewayToBridgeMsgData
import dev.bridgething.schema.GatewayToBridgePhoneMsg
import dev.bridgething.schema.InitiateCallType
import dev.bridgething.schema.PhoneAcceptAction
import dev.bridgething.schema.PhoneCall
import dev.bridgething.schema.PhoneCallAction
import dev.bridgething.schema.PhoneCallDirection
import dev.bridgething.schema.PhoneCallEnded
import dev.bridgething.schema.PhoneCallStatus
import dev.bridgething.schema.PhoneDtmfAction
import dev.bridgething.schema.PhoneEndAction
import dev.bridgething.schema.PhoneInitiateAction
import dev.bridgething.schema.PhoneMuteAction
import dev.bridgething.schema.PhoneState
import dev.bridgething.schema.CallEndReason
import io.mockk.mockk
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlin.time.Duration.Companion.seconds

/** phone dispatch: verifies each inbound verb reaches [PhoneBackend] with correct args and that backend events surface as wire frames. */
class PhoneDispatchTest {
    private suspend fun boot(scope: CoroutineScope, backend: PhoneBackend): Pair<BridgethingCompanion, WireDriver> {
        val adapter = FakeAdapter()
        val companion = BridgethingCompanion(
            context = mockk(relaxed = true),
            adapter = adapter,
            lyricsResolver = FakeLyricsResolver(),
            host = HostInfo(appName = "phone-test", appVersion = "0.0.1", osName = "test"),
            geo = NoOpGeoSource,
            volume = NoOpVolumeSource,
            audio = NoOpAudioBackend,
            phone = backend,
        )
        companion.start()
        val driver = WireDriver(adapter)
        driver.start(scope)
        driver.connect()
        return companion to driver
    }

    @Test
    fun `every inbound verb routes to backend with args`() = runBlocking {
        val backend = FakePhoneBackend()
        val (companion, driver) = boot(this, backend)
        driver.send(phone(BridgeToGatewayPhoneMsg.Answer(PhoneCallAction(callId = "c1"))))
        driver.send(phone(BridgeToGatewayPhoneMsg.Accept(PhoneAcceptAction(callId = "c2", action = AcceptCallAction.EndAndAccept))))
        driver.send(phone(BridgeToGatewayPhoneMsg.Decline(PhoneCallAction(callId = "c3"))))
        driver.send(phone(BridgeToGatewayPhoneMsg.End(PhoneCallAction(callId = "c4"))))
        driver.send(phone(BridgeToGatewayPhoneMsg.EndTyped(PhoneEndAction(callId = "c5", action = EndCallAction.EndAll))))
        driver.send(phone(BridgeToGatewayPhoneMsg.Hold(PhoneCallAction(callId = "c6"))))
        driver.send(phone(BridgeToGatewayPhoneMsg.Unhold(PhoneCallAction(callId = "c7"))))
        driver.send(phone(BridgeToGatewayPhoneMsg.Initiate(PhoneInitiateAction(kind = InitiateCallType.Destination, destinationId = "+15551234"))))
        driver.send(phone(BridgeToGatewayPhoneMsg.Swap))
        driver.send(phone(BridgeToGatewayPhoneMsg.Merge))
        driver.send(phone(BridgeToGatewayPhoneMsg.Mute(PhoneMuteAction(mute = true))))
        driver.send(phone(BridgeToGatewayPhoneMsg.Dtmf(PhoneDtmfAction(callId = "c8", tone = DtmfTone.Star))))

        eventually {
            backend.answered == listOf("c1") &&
                backend.accepted == listOf("c2" to AcceptCallAction.EndAndAccept) &&
                backend.declined == listOf("c3") &&
                backend.ended == listOf("c4") &&
                backend.endedTyped == listOf("c5" to EndCallAction.EndAll) &&
                backend.held == listOf("c6") &&
                backend.unheld == listOf("c7") &&
                backend.initiated.map { it.destinationId } == listOf("+15551234") &&
                backend.swapCount.get() == 1 &&
                backend.mergeCount.get() == 1 &&
                backend.muted == listOf(true) &&
                backend.dtmfed == listOf("c8" to DtmfTone.Star)
        }
        companion.stop()
    }

    @Test
    fun `stateGet replies with backend state`() = runBlocking {
        val call = PhoneCall(
            callId = "c1",
            remoteId = "+15550100",
            displayName = "Ada",
            status = PhoneCallStatus.Active,
            direction = PhoneCallDirection.Incoming,
        )
        val backend = FakePhoneBackend(state = PhoneState(activeCalls = listOf(call)))
        val (companion, driver) = boot(this, backend)

        val reply = withTimeout(5.seconds) {
            driver.request(phone(BridgeToGatewayPhoneMsg.StateGet))
        }
        val inner = ((reply.data as GatewayToBridgeMsgData.Phone).data as GatewayToBridgePhoneMsg.StateReply).data
        assertEquals(listOf(call), inner.state.activeCalls)
        companion.stop()
    }

    @Test
    fun `backend events surface as wire frames`() = runBlocking {
        val backend = FakePhoneBackend()
        val (companion, driver) = boot(this, backend)
        val call = PhoneCall(
            callId = "c9",
            remoteId = "+15550199",
            displayName = "Grace",
            status = PhoneCallStatus.Ringing,
            direction = PhoneCallDirection.Incoming,
        )
        backend.emit(PhoneOutEvent.CallStarted(call))
        backend.emit(PhoneOutEvent.Snapshot(PhoneState(activeCalls = listOf(call))))
        backend.emit(PhoneOutEvent.CallEnded(PhoneCallEnded(callId = "c9", reason = CallEndReason.Remote)))
        backend.emit(PhoneOutEvent.Communications(CommunicationsState(carrierName = "Test Mobile")))

        withTimeout(5.seconds) {
            driver.waitOutbound { msg ->
                val inner = (msg.data as? GatewayToBridgeMsgData.Phone)?.data as? GatewayToBridgePhoneMsg.CallStarted
                inner?.data?.callId == "c9"
            }
        }
        val ended = withTimeout(5.seconds) {
            driver.waitOutbound { msg ->
                val inner = (msg.data as? GatewayToBridgeMsgData.Phone)?.data as? GatewayToBridgePhoneMsg.CallEnded
                inner?.data?.callId == "c9"
            }
        }
        assertEquals(CallEndReason.Remote, ((ended.data as GatewayToBridgeMsgData.Phone).data as GatewayToBridgePhoneMsg.CallEnded).data.reason)

        val comms = withTimeout(5.seconds) {
            driver.waitOutbound { msg ->
                (msg.data as? GatewayToBridgeMsgData.Phone)?.data is GatewayToBridgePhoneMsg.CommunicationsSnapshot
            }
        }
        assertEquals("Test Mobile", ((comms.data as GatewayToBridgeMsgData.Phone).data as GatewayToBridgePhoneMsg.CommunicationsSnapshot).data.state.carrierName)
        companion.stop()
    }

    private fun phone(msg: BridgeToGatewayPhoneMsg): BridgeToGatewayMsgData =
        BridgeToGatewayMsgData.Phone(msg)

    private suspend fun eventually(predicate: () -> Boolean) {
        repeat(300) {
            if (predicate()) return
            delay(10)
        }
        assertEquals(true, predicate(), "predicate did not hold within the deadline")
    }
}

class FakePhoneBackend(private val state: PhoneState = PhoneState(activeCalls = emptyList())) : PhoneBackend {
    private val flow = MutableSharedFlow<PhoneOutEvent>(replay = 16)
    override val events: Flow<PhoneOutEvent> = flow

    val answered = CopyOnWriteArrayList<String>()
    val accepted = CopyOnWriteArrayList<Pair<String, AcceptCallAction>>()
    val declined = CopyOnWriteArrayList<String>()
    val ended = CopyOnWriteArrayList<String>()
    val endedTyped = CopyOnWriteArrayList<Pair<String, EndCallAction>>()
    val held = CopyOnWriteArrayList<String>()
    val unheld = CopyOnWriteArrayList<String>()
    val initiated = CopyOnWriteArrayList<PhoneInitiateAction>()
    val muted = CopyOnWriteArrayList<Boolean>()
    val dtmfed = CopyOnWriteArrayList<Pair<String?, DtmfTone>>()
    val swapCount = AtomicInteger(0)
    val mergeCount = AtomicInteger(0)

    suspend fun emit(event: PhoneOutEvent) = flow.emit(event)

    override suspend fun answer(callId: String) { answered.add(callId) }
    override suspend fun accept(callId: String, action: AcceptCallAction) { accepted.add(callId to action) }
    override suspend fun decline(callId: String) { declined.add(callId) }
    override suspend fun end(callId: String) { ended.add(callId) }
    override suspend fun endTyped(callId: String, action: EndCallAction) { endedTyped.add(callId to action) }
    override suspend fun hold(callId: String) { held.add(callId) }
    override suspend fun unhold(callId: String) { unheld.add(callId) }
    override suspend fun initiate(action: PhoneInitiateAction) { initiated.add(action) }
    override suspend fun swap() { swapCount.incrementAndGet() }
    override suspend fun merge() { mergeCount.incrementAndGet() }
    override suspend fun mute(muted: Boolean) { this.muted.add(muted) }
    override suspend fun dtmf(callId: String?, tone: DtmfTone) { dtmfed.add(callId to tone) }
    override suspend fun stateGet(): PhoneState = state
}
