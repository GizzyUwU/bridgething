package com.bridgething.companion

import android.util.Log
import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.GatewayEvent
import com.bridgething.gateway.device
import com.bridgething.schema.BridgeToGatewayMsgData
import com.bridgething.schema.BridgeToGatewayVoiceMsg
import com.bridgething.schema.NluStage
import com.bridgething.schema.VoiceCloseReason
import com.bridgething.schema.VoiceDispatch
import com.bridgething.schema.VoiceFormat
import com.bridgething.schema.VoiceFrame
import com.bridgething.schema.VoiceStreamClose
import com.bridgething.schema.VoiceStreamOpen
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

public class VoiceDispatcher(
    private val recognizer: NluSpeechRecognizing,
    private val decoder: VoicePacketDecoding,
    private val controller: VoiceController,
) {
    private class Capture(val format: VoiceFormat) {
        val packets: ConcurrentHashMap<UInt, ByteArray> = ConcurrentHashMap()
    }

    private class Turn(val streamId: UUID, val format: VoiceFormat, val packets: List<ByteArray>)

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val mutex = Mutex()
    private val jobs = mutableListOf<Job>()
    private val captures = ConcurrentHashMap<UUID, Capture>()

    @Volatile
    private var resolverProvider: (suspend () -> VoiceCatalogResolving?)? = null

    @Volatile
    private var prewarmed = false

    public fun setCatalogResolver(provider: suspend () -> VoiceCatalogResolving?) {
        resolverProvider = provider
    }

    public suspend fun start(gateway: BridgethingGateway) {
        mutex.withLock {
            stopJobs()
            jobs.add(
                scope.launch {
                    runCatching { recognizer.prepare() }
                        .onFailure { Log.w(TAG, "speech recognizer unavailable: ${it.message ?: it}") }
                },
            )
            jobs.add(scope.launch { runCaptures(gateway) })
        }
    }

    public suspend fun stop() {
        mutex.withLock {
            stopJobs()
            captures.clear()
            prewarmed = false
        }
    }

    private fun stopJobs() {
        for (job in jobs) job.cancel()
        jobs.clear()
    }

    private suspend fun runCaptures(gateway: BridgethingGateway): Unit = coroutineScope {
        gateway.events.collect { event ->
            if (event !is GatewayEvent.Message) return@collect
            val surface = event.message.data as? BridgeToGatewayMsgData.Voice ?: return@collect
            when (val msg = surface.data) {
                is BridgeToGatewayVoiceMsg.StreamOpen -> open(msg.data, this)
                is BridgeToGatewayVoiceMsg.Frame -> append(msg.data)
                is BridgeToGatewayVoiceMsg.StreamClose -> {
                    val turn = closeCapture(msg.data)
                    if (turn != null) launch { dispatch(turn, event.deviceId, gateway) }
                }
                else -> Unit
            }
        }
    }

    private fun open(msg: VoiceStreamOpen, scope: CoroutineScope) {
        captures[msg.streamId] = Capture(msg.format)
        if (prewarmed) return
        prewarmed = true
        scope.launch {
            runCatching { controller.prewarm() }
                .onFailure { Log.w(TAG, "prewarm failed: ${it.message ?: it}") }
        }
    }

    private fun append(msg: VoiceFrame) {
        captures[msg.streamId]?.packets?.put(msg.seq, msg.packet)
    }

    private fun closeCapture(msg: VoiceStreamClose): Turn? {
        val capture = captures.remove(msg.streamId) ?: return null
        if (msg.reason != VoiceCloseReason.EndOfSpeech) return null
        return Turn(
            streamId = msg.streamId,
            format = capture.format,
            packets = capture.packets.entries.sortedBy { it.key }.map { it.value },
        )
    }

    private suspend fun dispatch(turn: Turn, deviceId: String, gateway: BridgethingGateway) {
        resolveAndDispatch(transcribe(turn), deviceId, gateway)
    }

    private suspend fun transcribe(turn: Turn): String {
        if (turn.packets.isEmpty()) return ""
        return try {
            val samples = decoder.decode(turn.packets, turn.format)
            if (samples.isEmpty()) "" else recognizer.transcribe(samples, turn.format.sampleRateHz.toInt())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.w(TAG, "capture ${turn.streamId} failed: ${e.message ?: e}")
            ""
        }
    }

    private suspend fun resolveAndDispatch(transcript: String, deviceId: String, gateway: BridgethingGateway) {
        val resolution = try {
            controller.resolve(transcript)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.w(TAG, "nlu failed: ${e.message ?: e}")
            VoiceController.Resolution(
                NluPrediction(intent = NluIntentCatalog.NO_INTENT, transcript = transcript).toWire(),
                NluStage.NoModel,
            )
        }

        var prediction = NluPrediction.fromWire(resolution.resolved)
        val resolver = resolverProvider?.invoke()
        if (resolver != null) {
            prediction = try {
                resolver.decorate(prediction.copy(slots = prediction.slots.copy()))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.w(TAG, "catalog resolution failed, dispatching without a uri: ${e.message ?: e}")
                prediction
            }
        }

        gateway.device(deviceId).voice.dispatch(
            VoiceDispatch(resolved = prediction.toWire(), stage = resolution.stage),
        )
    }

    private companion object {
        const val TAG = "bridgething.voice"
    }
}
