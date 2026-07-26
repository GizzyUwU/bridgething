import AVFoundation
import Foundation
import Speech

@available(macOS 26.0, iOS 26.0, *)
public actor NluSpeechRecognizer {
    public struct Transcription: Sendable, Equatable {
        public let text: String
        public let alternatives: [String]
        public let confidence: Double?

        public init(text: String, alternatives: [String] = [], confidence: Double? = nil) {
            self.text = text
            self.alternatives = alternatives
            self.confidence = confidence
        }
    }

    public enum RecognizerError: Error, CustomStringConvertible {
        case localeUnsupported(Locale)
        case assetsUnavailable(Locale)
        case noSpeechDetected

        public var description: String {
            switch self {
            case let .localeUnsupported(locale):
                return "speech recognition does not support locale \(locale.identifier)"
            case let .assetsUnavailable(locale):
                return "speech model for \(locale.identifier) is not installed and could not be downloaded"
            case .noSpeechDetected:
                return "no speech found in the audio"
            }
        }
    }

    private let locale: Locale

    public init(locale: Locale = Locale(identifier: "en-US")) {
        self.locale = locale
    }

    private func makeTranscriber() -> SpeechTranscriber {
        SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            reportingOptions: [.alternativeTranscriptions],
            attributeOptions: [.transcriptionConfidence]
        )
    }

    public func prepare() async throws {
        let supported = await SpeechTranscriber.supportedLocales
        guard supported.contains(where: { $0.identifier(.bcp47) == locale.identifier(.bcp47) }) else {
            throw RecognizerError.localeUnsupported(locale)
        }

        let installed = await SpeechTranscriber.installedLocales
        if installed.contains(where: { $0.identifier(.bcp47) == locale.identifier(.bcp47) }) {
            return
        }

        let transcriber = makeTranscriber()
        guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) else {
            throw RecognizerError.assetsUnavailable(locale)
        }
        try await request.downloadAndInstall()
    }

    public func transcribe(fileAt url: URL) async throws -> Transcription {
        let file = try AVAudioFile(forReading: url)
        let transcriber = makeTranscriber()
        let analyzer = SpeechAnalyzer(modules: [transcriber])

        let collector = Task { () -> [SpeechTranscriber.Result] in
            var results: [SpeechTranscriber.Result] = []
            for try await result in transcriber.results {
                results.append(result)
            }
            return results
        }

        try await analyzer.start(inputAudioFile: file, finishAfterFile: true)
        let results = try await collector.value

        guard !results.isEmpty else { throw RecognizerError.noSpeechDetected }
        return Self.merge(results)
    }

    static func merge(_ results: [SpeechTranscriber.Result]) -> Transcription {
        var text = ""
        var alternatives: [String] = []
        var confidences: [Double] = []

        for result in results {
            text += String(result.text.characters)
            for run in result.text.runs {
                if let value = run.transcriptionConfidence {
                    confidences.append(value)
                }
            }
            if let best = result.alternatives.first {
                alternatives.append(String(best.characters))
            }
        }

        let merged = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return Transcription(
            text: merged,
            alternatives: alternatives.filter { !$0.isEmpty && $0 != merged },
            confidence: confidences.isEmpty ? nil : confidences.reduce(0, +) / Double(confidences.count)
        )
    }
}
