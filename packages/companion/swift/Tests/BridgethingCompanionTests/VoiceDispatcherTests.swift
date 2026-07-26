import BridgethingSchema
import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("voice dispatcher")
struct VoiceDispatcherTests {
    @available(macOS 26.0, iOS 26.0, *)
    @Test("wav header describes the pcm the daemon actually sent")
    func wavHeader() throws {
        let format = VoiceFormat(sampleRateHz: 16000, channels: 1, bitsPerSample: 16)
        let pcm = Data(repeating: 0xAB, count: 640)
        let url = try VoiceDispatcher.writeWav(pcm: pcm, format: format)
        defer { try? FileManager.default.removeItem(at: url) }

        let bytes = try Data(contentsOf: url)
        #expect(bytes.count == 44 + pcm.count)

        func u32(_ at: Int) -> UInt32 {
            bytes[at..<(at + 4)].reduce(into: UInt32(0)) { acc, b in acc = acc >> 8 | UInt32(b) << 24 }
        }
        func u16(_ at: Int) -> UInt16 {
            UInt16(bytes[at]) | UInt16(bytes[at + 1]) << 8
        }

        #expect(String(decoding: bytes[0..<4], as: UTF8.self) == "RIFF")
        #expect(String(decoding: bytes[8..<12], as: UTF8.self) == "WAVE")
        #expect(u16(22) == 1, "channel count")
        #expect(u32(24) == 16000, "sample rate")
        #expect(u16(34) == 16, "bits per sample")
        #expect(u32(28) == 16000 * 2, "byte rate")
        #expect(u16(32) == 2, "block align")
        #expect(u32(40) == UInt32(pcm.count), "data chunk size")
    }

    @available(macOS 26.0, iOS 26.0, *)
    @Test("stereo 32-bit format widens block align rather than assuming mono 16")
    func wavHeaderMultichannel() throws {
        let format = VoiceFormat(sampleRateHz: 48000, channels: 4, bitsPerSample: 32)
        let url = try VoiceDispatcher.writeWav(pcm: Data(repeating: 0, count: 64), format: format)
        defer { try? FileManager.default.removeItem(at: url) }
        let bytes = try Data(contentsOf: url)
        let blockAlign = UInt16(bytes[32]) | UInt16(bytes[33]) << 8
        #expect(blockAlign == 16, "4ch x 32bit is 16 bytes per frame")
    }
}
