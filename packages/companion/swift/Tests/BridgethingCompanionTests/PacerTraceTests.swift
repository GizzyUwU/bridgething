import XCTest

@testable import BridgethingCompanion

final class PacerTraceTests: XCTestCase {
    private final class VirtualClock {
        var seconds: Double = 0
    }

    func testEmitsPacerTrace() throws {
        let dir = fixturesDirectory()
        let corpus = try JSONDecoder().decode(
            Corpus.self,
            from: Data(contentsOf: dir.appendingPathComponent("pacer-trace.json"))
        )

        var cases: [EmittedCase] = []
        for testCase in corpus.cases {
            let clock = VirtualClock()
            var pacer = TransferPacer(startOffset: 0) { clock.seconds }
            var steps: [EmittedStep] = []

            for step in testCase.steps {
                clock.seconds = Double(step.tMs) / 1000.0
                if let acked = step.observe {
                    pacer.observe(ackedBytes: acked)
                }
                steps.append(
                    EmittedStep(
                        tMs: step.tMs,
                        windowBytes: pacer.windowBytes,
                        rateMicros: pacer.ratePerSec.map { Int64(($0 * 1e6).rounded()) }
                    )
                )
            }

            cases.append(EmittedCase(name: testCase.name, steps: steps))
        }

        let emitted = Emitted(
            impl: "swift",
            constants: Constants(
                targetDelayMs: UInt64((TransferPacer.targetDelaySeconds * 1000).rounded()),
                ackIntervalBytes: TransferPacer.ackIntervalBytes,
                minWindowBytes: TransferPacer.minWindowBytes,
                maxWindowBytes: TransferPacer.maxWindowBytes,
                rateSampleCount: TransferPacer.rateSampleCount,
                fragmentBytes: UInt64(TransferPacer.fragmentBytes)
            ),
            cases: cases
        )

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        var data = try encoder.encode(emitted)
        data.append(0x0A)
        try data.write(to: dir.appendingPathComponent("pacer-trace.swift.json"))
    }

    private func fixturesDirectory() -> URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // BridgethingCompanionTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // swift
            .deletingLastPathComponent()  // companion
            .deletingLastPathComponent()  // packages
            .deletingLastPathComponent()  // repo root
            .appendingPathComponent("crates/lib/fixtures")
            .standardizedFileURL
    }
}

// MARK: - corpus shapes

private struct Corpus: Decodable {
    let cases: [CorpusCase]
}

private struct CorpusCase: Decodable {
    let name: String
    let steps: [CorpusStep]
}

private struct CorpusStep: Decodable {
    let tMs: UInt64
    let observe: UInt64?

    enum CodingKeys: String, CodingKey {
        case tMs = "t_ms"
        case observe
    }
}

private struct Emitted: Encodable {
    let impl: String
    let constants: Constants
    let cases: [EmittedCase]
}

private struct Constants: Encodable {
    let targetDelayMs: UInt64
    let ackIntervalBytes: UInt64
    let minWindowBytes: UInt64
    let maxWindowBytes: UInt64
    let rateSampleCount: Int
    let fragmentBytes: UInt64?

    enum CodingKeys: String, CodingKey {
        case targetDelayMs = "target_delay_ms"
        case ackIntervalBytes = "ack_interval_bytes"
        case minWindowBytes = "min_window_bytes"
        case maxWindowBytes = "max_window_bytes"
        case rateSampleCount = "rate_sample_count"
        case fragmentBytes = "fragment_bytes"
    }
}

private struct EmittedCase: Encodable {
    let name: String
    let steps: [EmittedStep]
}

private struct EmittedStep: Encodable {
    let tMs: UInt64
    let windowBytes: UInt64
    let rateMicros: Int64?

    enum CodingKeys: String, CodingKey {
        case tMs = "t_ms"
        case windowBytes = "window_bytes"
        case rateMicros = "rate_micros"
    }
}
