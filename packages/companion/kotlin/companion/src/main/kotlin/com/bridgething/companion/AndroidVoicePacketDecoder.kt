package com.bridgething.companion

import android.media.MediaCodec
import android.media.MediaFormat
import com.bridgething.schema.VoiceCodec
import com.bridgething.schema.VoiceFormat
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

public class AndroidVoicePacketDecoder(
    private val dispatcher: CoroutineDispatcher = Dispatchers.Default,
) : VoicePacketDecoding {
    public override suspend fun decode(packets: List<ByteArray>, format: VoiceFormat): FloatArray {
        if (packets.isEmpty()) return FloatArray(0)
        return withContext(dispatcher) { run(packets, format) }
    }

    private fun run(packets: List<ByteArray>, format: VoiceFormat): FloatArray {
        val mime = when (format.codec) {
            VoiceCodec.Opus -> MediaFormat.MIMETYPE_AUDIO_OPUS
        }
        val rate = format.sampleRateHz.toInt()
        val channels = format.channels.toInt()

        val codec = MediaCodec.createDecoderByType(mime)
        val pcm = ByteArrayOutputStream()
        var outRate = rate
        var outChannels = channels
        try {
            codec.configure(headerFor(mime, rate, channels), null, null, 0)
            codec.start()

            val info = MediaCodec.BufferInfo()
            var next = 0
            var queuedEos = false
            var ptsUs = 0L
            var done = false
            while (!done) {
                if (!queuedEos) {
                    val index = codec.dequeueInputBuffer(TIMEOUT_US)
                    if (index >= 0) {
                        val buffer = codec.getInputBuffer(index) ?: error("decoder gave no input buffer")
                        if (next < packets.size) {
                            val packet = packets[next++]
                            buffer.clear()
                            buffer.put(packet)
                            codec.queueInputBuffer(index, 0, packet.size, ptsUs, 0)
                            ptsUs += PACKET_PTS_STEP_US
                        } else {
                            codec.queueInputBuffer(index, 0, 0, ptsUs, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                            queuedEos = true
                        }
                    }
                }

                val index = codec.dequeueOutputBuffer(info, TIMEOUT_US)
                if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    val actual = codec.outputFormat
                    outRate = actual.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                    outChannels = actual.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                } else if (index >= 0) {
                    val buffer = codec.getOutputBuffer(index) ?: error("decoder gave no output buffer")
                    if (info.size > 0) {
                        val chunk = ByteArray(info.size)
                        buffer.position(info.offset)
                        buffer.get(chunk)
                        pcm.write(chunk)
                    }
                    codec.releaseOutputBuffer(index, false)
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) done = true
                }
            }
            codec.stop()
        } finally {
            codec.release()
        }

        return resample(mono(pcm.toByteArray(), outChannels), outRate, rate)
    }

    private fun headerFor(mime: String, rate: Int, channels: Int): MediaFormat =
        MediaFormat.createAudioFormat(mime, rate, channels).apply {
            setByteBuffer(CSD_IDENTIFICATION, ByteBuffer.wrap(opusHead(rate, channels)))
            setByteBuffer(CSD_CODEC_DELAY, ByteBuffer.wrap(nanos(0)))
            setByteBuffer(CSD_SEEK_PREROLL, ByteBuffer.wrap(nanos(SEEK_PREROLL_NS)))
        }

    private fun opusHead(rate: Int, channels: Int): ByteArray =
        ByteBuffer.allocate(OPUS_HEAD_BYTES).order(ByteOrder.LITTLE_ENDIAN).apply {
            put(OPUS_HEAD_MAGIC.toByteArray(Charsets.US_ASCII))
            put(1)
            put(channels.toByte())
            putShort(0)
            putInt(rate)
            putShort(0)
            put(0)
        }.array()

    private fun nanos(value: Long): ByteArray =
        ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(value).array()

    private fun mono(pcm: ByteArray, channels: Int): FloatArray {
        val samples = ByteBuffer.wrap(pcm).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        val frames = samples.remaining() / channels
        val out = FloatArray(frames)
        for (frame in 0 until frames) {
            var sum = 0f
            for (channel in 0 until channels) sum += samples.get(frame * channels + channel) / FULL_SCALE
            out[frame] = sum / channels
        }
        return out
    }

    private fun resample(samples: FloatArray, from: Int, to: Int): FloatArray {
        if (from == to || samples.isEmpty()) return samples
        val count = (samples.size.toLong() * to / from).toInt()
        val out = FloatArray(count)
        val step = from.toDouble() / to.toDouble()
        for (i in 0 until count) {
            val at = i * step
            val index = at.toInt()
            val fraction = (at - index).toFloat()
            val here = samples[index]
            val next = if (index + 1 < samples.size) samples[index + 1] else here
            out[i] = here + (next - here) * fraction
        }
        return out
    }

    private companion object {
        const val CSD_IDENTIFICATION = "csd-0"
        const val CSD_CODEC_DELAY = "csd-1"
        const val CSD_SEEK_PREROLL = "csd-2"
        const val OPUS_HEAD_MAGIC = "OpusHead"
        const val OPUS_HEAD_BYTES = 19
        const val SEEK_PREROLL_NS = 80_000_000L
        const val TIMEOUT_US = 10_000L
        const val PACKET_PTS_STEP_US = 20_000L
        const val FULL_SCALE = 32768f
    }
}
