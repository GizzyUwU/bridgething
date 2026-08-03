package com.bridgething.companion

interface VoiceCatalogResolving {
    suspend fun decorate(prediction: NluPrediction): NluPrediction
}

interface VoiceCatalogProviding {
    fun voiceResolver(): VoiceCatalogResolving?
}
