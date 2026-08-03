import AVFoundation
import BridgethingSchema
import Foundation

final class VoicePacketDecoder {
    enum Failure: Error {
        case converterUnavailable
        case bufferUnavailable
        case decodeFailed(any Error)
    }

    private let output: AVAudioFormat
    private let input: AVAudioFormat
    private let converter: AVAudioConverter

    private static let drainCapacity: AVAudioFrameCount = 4096

    init(format: VoiceFormat) throws {
        let codec: AudioFormatID = switch format.codec {
        case .opus: kAudioFormatOpus
        }
        var asbd = AudioStreamBasicDescription(
            mSampleRate: Float64(format.sampleRateHz),
            mFormatID: codec,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: 0,
            mBytesPerFrame: 0,
            mChannelsPerFrame: UInt32(format.channels),
            mBitsPerChannel: 0,
            mReserved: 0
        )
        guard let input = AVAudioFormat(streamDescription: &asbd),
              let output = AVAudioFormat(
                  commonFormat: .pcmFormatInt16,
                  sampleRate: Double(format.sampleRateHz),
                  channels: AVAudioChannelCount(format.channels),
                  interleaved: true
              ),
              let converter = AVAudioConverter(from: input, to: output)
        else { throw Failure.converterUnavailable }

        self.input = input
        self.output = output
        self.converter = converter
    }

    func decode(_ packets: [Data]) throws -> Data {
        guard !packets.isEmpty else { return Data() }

        var next = 0
        var pcm = Data()
        var status = AVAudioConverterOutputStatus.haveData

        while status == .haveData {
            guard let buffer = AVAudioPCMBuffer(pcmFormat: output, frameCapacity: Self.drainCapacity) else {
                throw Failure.bufferUnavailable
            }
            var error: NSError?
            status = converter.convert(to: buffer, error: &error) { [input] _, request in
                guard next < packets.count else {
                    request.pointee = .endOfStream
                    return nil
                }
                let packet = packets[next]
                next += 1
                let compressed = AVAudioCompressedBuffer(
                    format: input, packetCapacity: 1, maximumPacketSize: packet.count
                )
                packet.withUnsafeBytes { bytes in
                    _ = memcpy(compressed.data, bytes.baseAddress!, packet.count)
                }
                compressed.byteLength = UInt32(packet.count)
                compressed.packetCount = 1
                compressed.packetDescriptions?.pointee = AudioStreamPacketDescription(
                    mStartOffset: 0, mVariableFramesInPacket: 0, mDataByteSize: UInt32(packet.count)
                )
                request.pointee = .haveData
                return compressed
            }
            if let error { throw Failure.decodeFailed(error) }
            if let samples = buffer.int16ChannelData, buffer.frameLength > 0 {
                let count = Int(buffer.frameLength) * Int(output.channelCount)
                pcm.append(UnsafeBufferPointer(start: samples[0], count: count))
            }
        }
        return pcm
    }
}
