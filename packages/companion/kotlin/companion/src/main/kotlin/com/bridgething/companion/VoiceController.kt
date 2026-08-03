package com.bridgething.companion

import com.bridgething.schema.NluAlternate
import com.bridgething.schema.NluResolvedIntent
import com.bridgething.schema.NluStage
import kotlinx.coroutines.CancellationException

class VoiceController(
    private val client: NluInferring? = null,
    private val config: Config = Config(),
) {
    data class Config(
        val useFastPath: Boolean = true,
        val rejection: NluRejectionPolicy = NluRejectionPolicy(),
    )

    data class Resolution(val resolved: NluResolvedIntent, val stage: NluStage)

    class InferenceFailed(cause: Throwable) : Exception("nlu inference failed: $cause", cause)

    suspend fun prewarm() {
        (client as? NluPrewarmable)?.prewarm()
    }

    suspend fun resolve(transcript: String): Resolution {
        val trimmed = transcript.trim()
        if (trimmed.isEmpty()) return noIntent(transcript, NluStage.RejectedNoIntent)

        if (config.useFastPath) {
            NluFastPath.match(trimmed)?.let { hit ->
                val pred = NluPrediction(intent = hit.intent, transcript = transcript, slots = hit.slots)
                return Resolution(pred.toWire(), NluStage.FastPath)
            }
        }

        val client = client ?: return noIntent(transcript, NluStage.NoModel)

        val output = try {
            client.infer(trimmed)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            throw InferenceFailed(e)
        }

        return when (val outcome = NluRejection.evaluate(output, config.rejection)) {
            is NluRejectionOutcome.NoIntent -> noIntent(transcript, NluStage.RejectedNoIntent)

            is NluRejectionOutcome.Clarify -> {
                val pred = NluPrediction(
                    intent = NluIntentCatalog.CLARIFY,
                    transcript = transcript,
                    alternates = outcome.alternates.map { NluAlternate(intent = it, slots = null) },
                )
                Resolution(pred.toWire(), NluStage.RejectedClarify)
            }

            is NluRejectionOutcome.Accept -> {
                val pred = NluPrediction(
                    intent = outcome.intent,
                    transcript = transcript,
                    slots = NluMutableSlots.fromWire(output.slots),
                )
                Resolution(pred.toWire(), NluStage.Model)
            }
        }
    }

    private fun noIntent(transcript: String, stage: NluStage): Resolution =
        Resolution(
            NluPrediction(intent = NluIntentCatalog.NO_INTENT, transcript = transcript).toWire(),
            stage,
        )
}
