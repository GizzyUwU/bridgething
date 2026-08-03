package com.bridgething.nlukit

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Assumptions.assumeTrue
import org.junit.jupiter.api.DisplayName
import org.junit.jupiter.api.Test

class NluIntentCatalogTest {
    @Test
    @DisplayName("catalog matches the decoding grammar")
    fun matchesGrammar() {
        val file = NluFixtures.grammar
        assumeTrue(file.isFile, "decoding grammar not present")

        val branches = Json.parseToJsonElement(file.readText()).jsonObject.getValue("oneOf").jsonArray
        val grammarIntents = branches.mapNotNull {
            it.jsonObject["properties"]?.jsonObject?.get("intent")?.jsonObject?.get("const")?.jsonPrimitive?.content
        }.toSet()

        assertTrue(grammarIntents.isNotEmpty(), "could not parse intents out of the grammar")
        val catalog = NluIntentCatalog.surfaceNames.toSet()
        assertEquals(
            emptySet<String>(),
            grammarIntents - catalog,
            "grammar admits intents the catalog never lists",
        )
        assertEquals(
            emptySet<String>(),
            catalog - grammarIntents,
            "catalog lists intents the grammar rejects",
        )
    }

    @Test
    @DisplayName("label indices are unique and stable")
    fun labelIndices() {
        val names = NluIntentCatalog.surfaceNames
        assertEquals(names.size, names.toSet().size, "duplicate intent names would collide label indices")
        assertEquals(names.sorted(), names, "label order must stay alphabetical to match the exported head")
        assertEquals(names.first(), NluIntentCatalog.name(0))
        assertNull(NluIntentCatalog.name(names.size))
    }

    @Test
    @DisplayName("rejection wire values are not model classes")
    fun rejectionValuesExcluded() {
        assertFalse(NluIntentCatalog.contains(NluIntentCatalog.NO_INTENT))
        assertFalse(NluIntentCatalog.contains(NluIntentCatalog.CLARIFY))
    }

    @Test
    @DisplayName("catalog matches the exported bundle manifest")
    fun matchesBundleManifest() {
        val file = NluFixtures.bundleDir.resolve("manifest.json")
        assumeTrue(file.isFile, "bundle manifest not present")

        val manifest = Json.parseToJsonElement(file.readText()).jsonObject
        val intents = manifest.getValue("intents").jsonArray.map {
            it.jsonObject.getValue("name").jsonPrimitive.content
        }
        assertEquals(NluIntentCatalog.surfaceNames, intents)
    }
}
