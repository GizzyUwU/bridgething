package com.bridgething.companion

import com.bridgething.schema.VoiceFormat

public interface VoicePacketDecoding {
    public suspend fun decode(packets: List<ByteArray>, format: VoiceFormat): FloatArray
}
