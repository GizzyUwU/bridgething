package com.bridgething.companion.shell

import java.io.File
import java.nio.file.Files
import org.junit.jupiter.api.Assertions.assertDoesNotThrow
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Test
import uniffi.bridgething_companion.ModelArtifactKind
import uniffi.bridgething_companion.ModelValidationException

class LitertArtifactValidatorTest {
    private fun temp(bytes: ByteArray): File {
        val file = Files.createTempFile("bridgething-asr", ".bin").toFile()
        file.deleteOnExit()
        file.writeBytes(bytes)
        return file
    }

    @Test
    fun asrWeightsWithAGgmlHeaderPass() {
        val file = temp(byteArrayOf(0x6c, 0x6d, 0x67, 0x67, 0, 1, 2, 3))
        val validator = LitertArtifactValidator()
        assertDoesNotThrow { validator.validate(ModelArtifactKind.ASR_MODEL, file.absolutePath) }
    }

    @Test
    fun asrWeightsWithoutTheHeaderAreInvalid() {
        val file = temp(byteArrayOf(1, 2, 3, 4, 5))
        val validator = LitertArtifactValidator()
        assertThrows(ModelValidationException::class.java) {
            validator.validate(ModelArtifactKind.ASR_MODEL, file.absolutePath)
        }
    }

    @Test
    fun truncatedAsrWeightsAreInvalid() {
        val file = temp(byteArrayOf(0x6c))
        val validator = LitertArtifactValidator()
        assertThrows(ModelValidationException::class.java) {
            validator.validate(ModelArtifactKind.ASR_MODEL, file.absolutePath)
        }
    }

    @Test
    fun aStagedNluBundleWithoutAModelIsInvalid() {
        val dir = Files.createTempDirectory("bridgething-nlu").toFile()
        dir.deleteOnExit()
        val validator = LitertArtifactValidator()
        assertThrows(ModelValidationException::class.java) {
            validator.validate(ModelArtifactKind.NLU_MODEL, dir.absolutePath)
        }
    }
}
