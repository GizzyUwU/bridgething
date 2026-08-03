package com.bridgething.companion

public interface NluSpeechRecognizing {
    public suspend fun prepare() {}

    public suspend fun transcribe(samples: FloatArray, sampleRateHz: Int): String
}
