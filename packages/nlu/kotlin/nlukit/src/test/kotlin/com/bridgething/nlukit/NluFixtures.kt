package com.bridgething.nlukit

import java.io.File

object NluFixtures {
    val bundleDir: File =
        File(System.getProperty("user.home"), "Documents/carthing/nlu/results/encoder/ettin-68m-s2-bundle")

    val grammar: File = File(System.getProperty("user.home"), "Documents/carthing/nlu/configs/grammar.strict.json")

    fun load(): List<NluFixture> {
        val file = File(bundleDir, "fixtures.jsonl")
        return if (file.isFile) NluFixture.parseAll(file.readText()) else emptyList()
    }
}
