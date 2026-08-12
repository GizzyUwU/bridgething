package com.bridgething.companion.shell

import java.io.File
import uniffi.bridgething_companion.ModelArtifactKind
import uniffi.bridgething_companion.ModelArtifactValidator
import uniffi.bridgething_companion.ModelValidationException

public class LitertArtifactValidator : ModelArtifactValidator {
    override fun validate(kind: ModelArtifactKind, path: String) {
        when (kind) {
            ModelArtifactKind.NLU_MODEL -> validateNlu(File(path))
            ModelArtifactKind.ASR_MODEL -> requireGgml(File(path))
        }
    }

    private fun validateNlu(dir: File) {
        val model = File(dir, "model.tflite")
        if (!model.isFile) {
            throw ModelValidationException.Invalid("staged nlu bundle has no model.tflite")
        }
        try {
            LitertNluModel.load(model).close()
        } catch (t: Throwable) {
            throw ModelValidationException.Invalid(t.message ?: t.toString())
        }
    }

    private fun requireGgml(file: File) {
        val head = ByteArray(GGML_MAGIC.size)
        val read = try {
            file.inputStream().use { it.read(head) }
        } catch (t: Throwable) {
            throw ModelValidationException.Invalid(t.message ?: t.toString())
        }
        if (read != head.size || !head.contentEquals(GGML_MAGIC)) {
            throw ModelValidationException.Invalid("asr model does not open with a ggml header")
        }
    }

    private companion object {
        val GGML_MAGIC = byteArrayOf(0x6c, 0x6d, 0x67, 0x67)
    }
}
