#if canImport(Speech)

    import AVFoundation
    import BridgethingCompanionCore
    import Foundation
    import Speech

    public final class SpeechTranscriberBackend: SpeechRecognizer, @unchecked Sendable {
        private let locale: Locale

        public init(locale: Locale = Locale(identifier: "en-US")) {
            self.locale = locale
        }

        public func prepare(sink: PrepareSink) {
            let locale = locale
            Task {
                guard #available(iOS 26.0, macOS 26.0, *) else {
                    sink.onFailed(reason: "speech transcription needs ios 26")
                    return
                }
                do {
                    try await TranscriberEngine.prepare(locale: locale)
                    sink.onReady()
                } catch {
                    sink.onFailed(reason: String(describing: error))
                }
            }
        }

        public func transcribe(pcm: [Float], sampleRateHz: UInt32, sink: TranscriptionSink) {
            let locale = locale
            Task {
                guard #available(iOS 26.0, macOS 26.0, *) else {
                    sink.fail(reason: "speech transcription needs ios 26")
                    return
                }
                let url: URL
                do {
                    url = try Self.writeWav(pcm: pcm, sampleRateHz: sampleRateHz)
                } catch {
                    sink.fail(reason: "could not stage audio: \(String(describing: error))")
                    return
                }
                defer { try? FileManager.default.removeItem(at: url) }
                do {
                    let result = try await TranscriberEngine.transcribe(fileAt: url, locale: locale)
                    sink.complete(transcription: result)
                } catch {
                    sink.fail(reason: String(describing: error))
                }
            }
        }

        static func writeWav(pcm: [Float], sampleRateHz: UInt32) throws -> URL {
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("bridgething-voice-\(UUID().uuidString).wav")
            let settings: [String: Any] = [
                AVFormatIDKey: kAudioFormatLinearPCM,
                AVSampleRateKey: Double(sampleRateHz),
                AVNumberOfChannelsKey: 1,
                AVLinearPCMBitDepthKey: 32,
                AVLinearPCMIsFloatKey: true,
                AVLinearPCMIsNonInterleaved: false,
            ]
            let file = try AVAudioFile(forWriting: url, settings: settings)
            guard
                let format = AVAudioFormat(
                    commonFormat: .pcmFormatFloat32, sampleRate: Double(sampleRateHz),
                    channels: 1, interleaved: false
                ),
                let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(max(pcm.count, 1)))
            else {
                throw TranscribeStageError.bufferAllocation
            }
            buffer.frameLength = AVAudioFrameCount(pcm.count)
            if let channel = buffer.floatChannelData?[0] {
                pcm.withUnsafeBufferPointer { src in
                    if let base = src.baseAddress {
                        channel.update(from: base, count: pcm.count)
                    }
                }
            }
            try file.write(from: buffer)
            return url
        }

        enum TranscribeStageError: Error {
            case bufferAllocation
        }
    }

    @available(iOS 26.0, macOS 26.0, *)
    private enum TranscriberEngine {
        enum EngineError: Error, CustomStringConvertible {
            case localeUnsupported(Locale)
            case assetsUnavailable(Locale)
            case noSpeechDetected

            var description: String {
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

        private static func makeTranscriber(locale: Locale) -> SpeechTranscriber {
            SpeechTranscriber(
                locale: locale,
                transcriptionOptions: [],
                reportingOptions: [.alternativeTranscriptions],
                attributeOptions: [.transcriptionConfidence]
            )
        }

        static func prepare(locale: Locale) async throws {
            let supported = await SpeechTranscriber.supportedLocales
            guard supported.contains(where: { $0.identifier(.bcp47) == locale.identifier(.bcp47) }) else {
                throw EngineError.localeUnsupported(locale)
            }

            let installed = await SpeechTranscriber.installedLocales
            if installed.contains(where: { $0.identifier(.bcp47) == locale.identifier(.bcp47) }) {
                return
            }

            let transcriber = makeTranscriber(locale: locale)
            guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) else {
                throw EngineError.assetsUnavailable(locale)
            }
            try await request.downloadAndInstall()
        }

        static func transcribe(fileAt url: URL, locale: Locale) async throws -> Transcription {
            let file = try AVAudioFile(forReading: url)
            let transcriber = makeTranscriber(locale: locale)
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

            guard !results.isEmpty else { throw EngineError.noSpeechDetected }
            return merge(results)
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
                segments: [],
                confidence: confidences.isEmpty ? nil : Float(confidences.reduce(0, +) / Double(confidences.count))
            )
        }
    }

#endif
