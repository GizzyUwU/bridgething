import Foundation

public struct NluInferenceOutput: Sendable {
    public let intentLogits: [Double]
    public let inDomainLogit: Double
    public let slots: NluMutableSlots

    public init(intentLogits: [Double], inDomainLogit: Double, slots: NluMutableSlots = .init()) {
        self.intentLogits = intentLogits
        self.inDomainLogit = inDomainLogit
        self.slots = slots
    }
}

public protocol NluInferring: Sendable {
    func infer(transcript: String) async throws -> NluInferenceOutput
}

public protocol NluPrewarmable: Sendable {
    func prewarm() async
}
