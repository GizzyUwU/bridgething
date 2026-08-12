#if canImport(CoreML)

    import BridgethingCompanionCore
    import CoreML
    import Foundation

    public final class CoreMlArtifactValidator: ModelArtifactValidator, @unchecked Sendable {
        public init() {}

        public func validate(kind: ModelArtifactKind, path: String) throws {
            switch kind {
            case .nluModel:
                do {
                    _ = try CoreMlNluModel(bundleDir: URL(fileURLWithPath: path, isDirectory: true))
                } catch {
                    throw ModelValidationError.Invalid(message: String(describing: error))
                }
            case .asrModel:
                throw ModelValidationError.Invalid(message: "ios uses system speech models and stages no asr artifacts")
            }
        }
    }

#endif
