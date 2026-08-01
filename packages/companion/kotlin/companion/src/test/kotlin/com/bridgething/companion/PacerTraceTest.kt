package com.bridgething.companion

import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import kotlin.math.roundToLong
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Test

class PacerTraceTest {
  @Test
  fun `emits pacer trace`() {
    val dir = fixturesDirectory()
    val reader = Json { ignoreUnknownKeys = true }
    val corpus = reader.decodeFromString(Corpus.serializer(), dir.resolve("pacer-trace.json").toFile().readText())

    val cases = corpus.cases.map { case ->
      var seconds = 0.0
      val pacer = TransferPacer(startOffset = 0L) { seconds }
      val steps = case.steps.map { step ->
        seconds = step.tMs / 1000.0
        step.observe?.let { pacer.observe(it) }
        EmittedStep(
          tMs = step.tMs,
          windowBytes = pacer.windowBytes,
          rateMicros = pacer.ratePerSec?.let { (it * 1e6).roundToLong() },
        )
      }
      EmittedCase(name = case.name, steps = steps)
    }

    val emitted = Emitted(
      impl = "kotlin",
      constants = Constants(
        targetDelayMs = (TransferPacer.TARGET_DELAY_SECONDS * 1000).roundToLong(),
        ackIntervalBytes = TransferPacer.ACK_INTERVAL_BYTES,
        minWindowBytes = TransferPacer.MIN_WINDOW_BYTES,
        maxWindowBytes = TransferPacer.MAX_WINDOW_BYTES,
        rateSampleCount = TransferPacer.RATE_SAMPLE_COUNT,
        fragmentBytes = TransferPacer.FRAGMENT_BYTES.toLong(),
      ),
      cases = cases,
    )

    val writer = Json { prettyPrint = true; explicitNulls = true }
    dir.resolve("pacer-trace.kotlin.json").toFile().writeText(writer.encodeToString(emitted) + "\n")
  }

  private fun fixturesDirectory(): Path {
    val here = Paths.get(System.getProperty("user.dir"))
    val candidates = listOf(
      here.resolve("crates/lib/fixtures"),
      here.resolve("../../../../crates/lib/fixtures"),
      here.resolve("../../../crates/lib/fixtures"),
      here.resolve("../../crates/lib/fixtures"),
      here.resolve("../crates/lib/fixtures"),
    )
    return candidates.firstOrNull { Files.exists(it.resolve("pacer-trace.json")) }
      ?: error("could not locate crates/lib/fixtures from $here (tried: ${candidates.joinToString()})")
  }
}

@Serializable
private data class Corpus(val cases: List<CorpusCase>)

@Serializable
private data class CorpusCase(val name: String, val steps: List<CorpusStep>)

@Serializable
private data class CorpusStep(@SerialName("t_ms") val tMs: Long, val observe: Long? = null)

@Serializable
private data class Emitted(val impl: String, val constants: Constants, val cases: List<EmittedCase>)

@Serializable
private data class Constants(
  @SerialName("target_delay_ms") val targetDelayMs: Long,
  @SerialName("ack_interval_bytes") val ackIntervalBytes: Long,
  @SerialName("min_window_bytes") val minWindowBytes: Long,
  @SerialName("max_window_bytes") val maxWindowBytes: Long,
  @SerialName("rate_sample_count") val rateSampleCount: Int,
  @SerialName("fragment_bytes") val fragmentBytes: Long?,
)

@Serializable
private data class EmittedCase(val name: String, val steps: List<EmittedStep>)

@Serializable
private data class EmittedStep(
  @SerialName("t_ms") val tMs: Long,
  @SerialName("window_bytes") val windowBytes: Long,
  @SerialName("rate_micros") val rateMicros: Long?,
)
