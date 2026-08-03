package com.bridgething.nlukit

import uniffi.nlu.DecodedFrame
import uniffi.nlu.ManifestInfo
import uniffi.nlu.NluDecoderInterface
import uniffi.nlu.Rejection
import uniffi.nlu.SlotValue
import uniffi.nlu.TokenizedInput

class FakeNluDecoder(
    private val intentNames: List<String> = NluIntentCatalog.surfaceNames,
    private val rejection: Rejection? = Rejection(inDomainThreshold = 0.5, clarifyMargin = 0.4),
    private val frame: DecodedFrame = DecodedFrame("PLAY", emptyList()),
    private val tokens: TokenizedInput = TokenizedInput(List(4) { it }, List(4) { 1 }, emptyList(), emptyList()),
) : NluDecoderInterface {
    var tokenized: String? = null
    var decodedTranscript: String? = null
    var decodedIntentLogits: List<Float>? = null
    var decodedBioLogits: List<Float>? = null
    var decodedClosedLogits: List<List<Float>>? = null

    override fun info() = ManifestInfo(
        schemaVersion = "0.3.1",
        maxLen = tokens.inputIds.size.toUInt(),
        intentNames = intentNames,
        bioTagCount = 13u,
        closedHeadSizes = List(16) { 2u },
        rejection = rejection,
    )

    override fun tokenize(transcript: String): TokenizedInput {
        tokenized = transcript
        return tokens
    }

    override fun decode(
        transcript: String,
        tokens: TokenizedInput,
        intentLogits: List<Float>,
        bioLogits: List<Float>,
        closedLogits: List<List<Float>>,
    ): DecodedFrame {
        decodedTranscript = transcript
        decodedIntentLogits = intentLogits
        decodedBioLogits = bioLogits
        decodedClosedLogits = closedLogits
        return frame
    }
}

class FakeNluModel(private val outputs: NluModelOutputs) : NluModelRunning {
    var sawInputIds: List<Int>? = null
    var sawAttentionMask: List<Int>? = null
    var calls = 0

    override fun predict(inputIds: List<Int>, attentionMask: List<Int>): NluModelOutputs {
        calls += 1
        sawInputIds = inputIds
        sawAttentionMask = attentionMask
        return outputs
    }
}

fun NluFixture.asModelOutputs() = NluModelOutputs(
    intentLogits = intentLogits,
    oodLogit = oodLogit,
    bioLogits = bioLogits,
    closedLogits = closedLogits,
)

fun slots(vararg pairs: Pair<String, String>) = pairs.map { SlotValue(it.first, it.second) }
