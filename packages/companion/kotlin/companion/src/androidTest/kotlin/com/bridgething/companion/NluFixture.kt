package com.bridgething.companion

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

data class NluFixture(
    val utterance: String,
    val inputIds: List<Int>,
    val attentionMask: List<Int>,
    val intentLogits: List<Float>,
    val oodLogit: Float,
    val bioLogits: List<Float>,
    val closedLogits: List<List<Float>>,
    val expectedIntent: String,
    val expectedSlots: Map<String, String>,
) {
    companion object {
        fun parseAll(jsonl: String): List<NluFixture> =
            jsonl.lineSequence().filter(String::isNotBlank).map { parse(Json.parseToJsonElement(it).jsonObject) }.toList()

        private fun parse(row: JsonObject): NluFixture {
            val expected = row.getValue("expected").jsonObject
            return NluFixture(
                utterance = row.getValue("utterance").jsonPrimitive.content,
                inputIds = row.ints("inputIds"),
                attentionMask = row.ints("attentionMask"),
                intentLogits = row.floats("intentLogits"),
                oodLogit = row.getValue("oodLogit").jsonPrimitive.float,
                bioLogits = row.floats("bioLogits"),
                closedLogits = row.getValue("closedLogits").jsonArray.map { head ->
                    head.jsonArray.map { it.jsonPrimitive.float }
                },
                expectedIntent = expected.getValue("intent").jsonPrimitive.content,
                expectedSlots = expected.getValue("slots").jsonObject.mapValues { it.value.jsonPrimitive.content },
            )
        }

        private fun JsonObject.ints(key: String) = getValue(key).jsonArray.map { it.jsonPrimitive.int }

        private fun JsonObject.floats(key: String) = getValue(key).jsonArray.map { it.jsonPrimitive.float }
    }
}
