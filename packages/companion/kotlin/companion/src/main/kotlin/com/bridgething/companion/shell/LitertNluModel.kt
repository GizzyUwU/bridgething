package com.bridgething.companion.shell

import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.channels.FileChannel
import org.tensorflow.lite.DataType
import org.tensorflow.lite.Interpreter
import uniffi.bridgething_companion.NluModelOutputs

internal class LitertNluModel(private val interpreter: Interpreter) : AutoCloseable {
    class ModelError(message: String) : Exception(message)

    private val signature: String
    private val idsInput: String
    private val maskInput: String
    private val intentOutput: String
    private val oodOutput: String
    private val bioOutput: String
    private val closedOutputs: List<String>
    private val outputBuffers: Map<String, ByteBuffer>
    private val idsBuffer: ByteBuffer
    private val maskBuffer: ByteBuffer
    private val idsType: DataType
    private val maskType: DataType

    val sequenceLength: Int

    init {
        signature = interpreter.signatureKeys.firstOrNull()
            ?: throw ModelError("model exposes no signature")

        val inputs = interpreter.getSignatureInputs(signature).toList()
        idsInput = requireInput(inputs, IDS_INPUT)
        maskInput = requireInput(inputs, MASK_INPUT)

        val outputs = interpreter.getSignatureOutputs(signature).toSet()
        intentOutput = requireOutput(outputs, outputName(INTENT_SLOT))
        oodOutput = requireOutput(outputs, outputName(OOD_SLOT))
        bioOutput = requireOutput(outputs, outputName(BIO_SLOT))
        closedOutputs = generateSequence(CLOSED_SLOT) { it + 1 }
            .map(::outputName)
            .takeWhile(outputs::contains)
            .toList()
        if (closedOutputs.size != outputs.size - CLOSED_SLOT) {
            throw ModelError("signature outputs $outputs are not a contiguous output_N run")
        }

        val idsTensor = interpreter.getInputTensorFromSignature(idsInput, signature)
        val maskTensor = interpreter.getInputTensorFromSignature(maskInput, signature)
        idsType = idsTensor.dataType()
        maskType = maskTensor.dataType()
        sequenceLength = idsTensor.shape().last()
        idsBuffer = direct(idsTensor.numBytes())
        maskBuffer = direct(maskTensor.numBytes())

        outputBuffers = (listOf(intentOutput, oodOutput, bioOutput) + closedOutputs).associateWith {
            val tensor = interpreter.getOutputTensorFromSignature(it, signature)
            if (tensor.dataType() != DataType.FLOAT32) {
                throw ModelError("output $it is ${tensor.dataType()}, expected FLOAT32")
            }
            direct(tensor.numBytes())
        }
    }

    @Synchronized
    fun predict(inputIds: List<Int>, attentionMask: List<Int>): NluModelOutputs {
        fill(idsBuffer, idsType, inputIds, IDS_INPUT)
        fill(maskBuffer, maskType, attentionMask, MASK_INPUT)

        outputBuffers.values.forEach { it.rewind() }
        interpreter.runSignature(mapOf(idsInput to idsBuffer, maskInput to maskBuffer), outputBuffers, signature)

        return NluModelOutputs(
            intentLogits = floats(intentOutput),
            oodLogit = floats(oodOutput).firstOrNull() ?: 0f,
            bioLogits = floats(bioOutput),
            closedLogits = closedOutputs.map(::floats),
        )
    }

    override fun close() = interpreter.close()

    private fun fill(buffer: ByteBuffer, type: DataType, values: List<Int>, name: String) {
        if (values.size != sequenceLength) {
            throw ModelError("$name got ${values.size} tokens, model is frozen at $sequenceLength")
        }
        buffer.rewind()
        when (type) {
            DataType.INT64 -> values.forEach { buffer.putLong(it.toLong()) }
            DataType.INT32 -> values.forEach { buffer.putInt(it) }
            else -> throw ModelError("$name is $type, expected INT32 or INT64")
        }
        buffer.rewind()
    }

    private fun floats(name: String): List<Float> {
        val buffer = outputBuffers.getValue(name)
        buffer.rewind()
        val floats = buffer.asFloatBuffer()
        return List(floats.remaining()) { floats.get(it) }
    }

    private fun requireInput(inputs: List<String>, name: String): String =
        inputs.firstOrNull { it == name } ?: throw ModelError("signature input $name missing, have $inputs")

    private fun requireOutput(outputs: Set<String>, name: String): String =
        if (name in outputs) name else throw ModelError("signature output $name missing, have $outputs")

    companion object {
        private const val IDS_INPUT = "args_0"
        private const val MASK_INPUT = "args_1"
        private const val INTENT_SLOT = 0
        private const val OOD_SLOT = 1
        private const val BIO_SLOT = 2
        private const val CLOSED_SLOT = 3

        private fun outputName(slot: Int) = "output_$slot"

        private fun direct(bytes: Int): ByteBuffer =
            ByteBuffer.allocateDirect(bytes).order(ByteOrder.nativeOrder())

        fun load(model: File, threads: Int = 2): LitertNluModel {
            val weights = RandomAccessFile(model, "r").use {
                it.channel.map(FileChannel.MapMode.READ_ONLY, 0, it.length())
            }
            return LitertNluModel(Interpreter(weights, Interpreter.Options().setNumThreads(threads)))
        }
    }
}
