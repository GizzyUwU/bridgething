package com.bridgething.companion

import com.bridgething.schema.NluView
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.Test
import java.io.File
import java.util.Locale

class NluFastPathAsrEvalTest {
    @Serializable
    data class Gold(val intent: String, val slots: Map<String, JsonPrimitive> = emptyMap())

    @Serializable
    data class Row(val id: String, val utterance: String, val reference: String, val gold: Gold)

    enum class Outcome { CORRECT, SLOTS_WRONG, INTENT_WRONG, DECLINED }

    class Tally {
        var correct = 0
        var slotsWrong = 0
        var intentWrong = 0
        var declined = 0

        val total: Int get() = correct + slotsWrong + intentWrong + declined

        fun add(o: Outcome) {
            when (o) {
                Outcome.CORRECT -> correct++
                Outcome.SLOTS_WRONG -> slotsWrong++
                Outcome.INTENT_WRONG -> intentWrong++
                Outcome.DECLINED -> declined++
            }
        }
    }

    companion object {
        val expressibleSlots: Map<String, Set<String>> = mapOf(
            "PLAY" to setOf("target_type"),
            "PAUSE" to emptySet(),
            "NEXT" to emptySet(),
            "PREVIOUS" to emptySet(),
            "SET_VOLUME" to setOf("level", "direction", "amount", "mute"),
            "SET_SHUFFLE" to setOf("enabled"),
            "SET_REPEAT" to setOf("repeat_mode"),
            "SET_PLAYBACK_SPEED" to setOf("speed"),
            "SEEK_RELATIVE" to setOf("seconds"),
            "PRESET_PLAY" to setOf("preset"),
            "PRESET_SAVE" to setOf("preset"),
            "SHOW_VIEW" to setOf("view"),
        )

        val scored = setOf("PAUSE", "NEXT", "PREVIOUS", "SET_SHUFFLE", "SET_REPEAT")

        private val json = Json { ignoreUnknownKeys = true }

        fun expressible(gold: Gold): Boolean {
            val allowed = expressibleSlots[gold.intent] ?: return false
            if (!gold.slots.keys.all { it in allowed }) return false
            if (gold.intent == "SHOW_VIEW") {
                val view = gold.slots["view"] ?: return false
                return view.isString && view.content == "now_playing"
            }
            return true
        }

        fun slotsAgree(hit: NluFastPath.Hit, gold: Gold): Boolean {
            for ((key, value) in gold.slots) {
                val text = if (value.isString) value.content else null
                val flag = if (value.isString) null else value.booleanOrNull
                val number = if (value.isString) null else value.intOrNull
                val agrees = when {
                    key == "repeat_mode" && text != null -> hit.slots.repeatMode?.string == text
                    key == "enabled" && flag != null -> hit.slots.enabled == flag
                    key == "mute" && flag != null -> hit.slots.mute == flag
                    key == "preset" && text != null -> hit.slots.preset == text
                    key == "level" && number != null -> hit.slots.level?.toInt() == number
                    key == "seconds" && number != null -> hit.slots.seconds == number
                    key == "speed" && text != null -> hit.slots.speed?.string == text
                    key == "direction" && text != null -> hit.slots.direction?.string == text
                    key == "amount" && text != null -> hit.slots.amount?.string == text
                    key == "view" && text == "now_playing" -> hit.slots.view == NluView.NowPlaying
                    key == "target_type" && text != null -> hit.intent == "PLAY"
                    else -> false
                }
                if (!agrees) return false
            }
            return true
        }

        fun score(transcript: String, gold: Gold): Outcome {
            val hit = NluFastPath.match(transcript) ?: return Outcome.DECLINED
            if (hit.intent != gold.intent) return Outcome.INTENT_WRONG
            return if (slotsAgree(hit, gold)) Outcome.CORRECT else Outcome.SLOTS_WRONG
        }

        fun pct(n: Int, d: Int): String =
            if (d == 0) "n/a" else String.format(Locale.ROOT, "%5.1f%%", 100.0 * n / d)

        fun loadRows(): List<Row>? {
            val path = System.getenv("BRIDGETHING_ASR_EVAL") ?: return null
            return File(path).readLines().mapNotNull { line ->
                val trimmed = line.trim()
                if (trimmed.isEmpty()) null else json.decodeFromString(Row.serializer(), trimmed)
            }
        }
    }

    private val lanes: List<Pair<String, (Row) -> String>> = listOf(
        "hypothesis (whisper)" to { r: Row -> r.utterance },
        "reference (clean)" to { r: Row -> r.reference },
    )

    @Test
    fun `scores the fast path on recognizer hypotheses and clean references`() {
        val rows = loadRows()
        assumeTrue(rows != null, "BRIDGETHING_ASR_EVAL unset; skipping")
        rows!!

        println("rows: ${rows.size}\n")

        for ((label, pick) in lanes) {
            val byIntent = sortedMapOf<String, Tally>()
            for (row in rows) {
                byIntent.getOrPut(row.gold.intent) { Tally() }.add(score(pick(row), row.gold))
            }

            println("== $label ==")
            println("  intent            n   correct  slotWrong intentWrong  declined")
            for ((intent, t) in byIntent) {
                println(
                    "  ${intent.padEnd(16)} ${String.format(Locale.ROOT, "%4d", t.total)}" +
                        "    ${pct(t.correct, t.total)}" +
                        "     ${pct(t.slotsWrong, t.total)}" +
                        "      ${pct(t.intentWrong, t.total)}" +
                        "     ${pct(t.declined, t.total)}",
                )
            }

            var recallHit = 0
            var recallTotal = 0
            var wrongFire = 0
            var mustNotFireTotal = 0
            var partialFire = 0
            for (row in rows) {
                val outcome = score(pick(row), row.gold)
                if (row.gold.intent in scored && expressible(row.gold)) {
                    recallTotal++
                    if (outcome == Outcome.CORRECT) recallHit++
                }
                if (!expressible(row.gold)) {
                    mustNotFireTotal++
                    when (outcome) {
                        Outcome.DECLINED -> Unit
                        Outcome.SLOTS_WRONG, Outcome.CORRECT -> partialFire++
                        Outcome.INTENT_WRONG -> wrongFire++
                    }
                }
            }
            val served = rows.count { score(pick(it), it.gold) == Outcome.CORRECT }
            val needsModel = rows.count {
                it.gold.intent != "NO_INTENT" && score(pick(it), it.gold) != Outcome.CORRECT
            }

            println("")
            println("  END TO END served correctly: $served/${rows.size} ${pct(served, rows.size)}")
            println("  real commands needing the model: $needsModel/${rows.size} ${pct(needsModel, rows.size)}")
            println("  recall on scored classes: $recallHit/$recallTotal ${pct(recallHit, recallTotal)}")
            println("  wrong fire on must-decline: $wrongFire/$mustNotFireTotal ${pct(wrongFire, mustNotFireTotal)}")
            println("  partial fire (same intent, slot underserved): $partialFire/$mustNotFireTotal")
            println("")

            if (label.startsWith("hypothesis")) {
                assertEquals(0, wrongFire, "$label: fast path claimed $wrongFire utterances it cannot serve")
                assertTrue(partialFire <= 2, "$label: same-intent partial fires grew past the measured count")
            }
            assertTrue(
                recallHit.toDouble() / recallTotal >= 0.65,
                "$label: recall regressed below the measured floor",
            )
        }
    }

    @Test
    fun `lists every fire the fast path should have declined`() {
        val rows = loadRows()
        assumeTrue(rows != null, "BRIDGETHING_ASR_EVAL unset; skipping")
        rows!!

        for ((label, pick) in lanes) {
            println("== fires on must-decline, $label ==")
            var shown = 0
            for (row in rows) {
                if (expressible(row.gold)) continue
                val text = pick(row)
                val hit = NluFastPath.match(text) ?: continue
                shown++
                println("  gold=${row.gold.intent} got=${hit.intent}  \"$text\"")
            }
            println("  total: $shown")
        }
    }

    @Test
    fun `lists misses on the scored reachable classes`() {
        val rows = loadRows()
        assumeTrue(rows != null, "BRIDGETHING_ASR_EVAL unset; skipping")
        rows!!

        println("== misses on scored classes (recognizer hypotheses) ==")
        val counts = sortedMapOf<String, Int>()
        for (row in rows) {
            if (row.gold.intent !in scored || !expressible(row.gold)) continue
            if (score(row.utterance, row.gold) == Outcome.CORRECT) continue
            counts[row.gold.intent] = (counts[row.gold.intent] ?: 0) + 1
            val blame = if (score(row.reference, row.gold) == Outcome.CORRECT) "asr" else "rules"
            println("  [$blame] gold=${row.gold.intent} \"${row.utterance}\"")
        }
        println("  by intent: ${counts.entries.map { it.key to it.value }}")
    }
}
