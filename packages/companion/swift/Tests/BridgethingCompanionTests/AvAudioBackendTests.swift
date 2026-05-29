import Foundation
import XCTest

@testable import BridgethingCompanion

#if canImport(AVFoundation)
    import AVFoundation

    final class AvAudioBackendTests: XCTestCase {
        func testRealSynthesisRendersNonEmptyPcm() async throws {
            let synth = AVSpeechSynthesizer()
            let utterance = AVSpeechUtterance(string: "bridgething audio check")
            let collector = FrameCounter()
            let done = expectation(description: "synthesis render complete")
            // the synthesizer may emit more than one terminal zero-length buffer.
            done.assertForOverFulfill = false

            synth.write(utterance) { buffer in
                guard let pcm = buffer as? AVAudioPCMBuffer else { return }
                if pcm.frameLength == 0 {
                    // a terminal zero-length buffer signals the end of the stream.
                    done.fulfill()
                } else {
                    collector.add(Int(pcm.frameLength))
                }
            }

            await fulfillment(of: [done], timeout: 15)
            XCTAssertGreaterThan(collector.total, 0, "real TTS should render non-silent PCM frames")
        }
    }

    private final class FrameCounter: @unchecked Sendable {
        private let lock = NSLock()
        private var count = 0

        func add(_ n: Int) {
            lock.lock()
            count += n
            lock.unlock()
        }

        var total: Int {
            lock.lock()
            defer { lock.unlock() }
            return count
        }
    }
#endif
