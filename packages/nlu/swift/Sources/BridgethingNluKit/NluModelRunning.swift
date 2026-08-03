import Foundation

public struct NluModelOutputs: Sendable {
    public let intentLogits: [Float]
    public let oodLogit: Float
    public let bioLogits: [Float]
    public let closedLogits: [[Float]]

    public init(intentLogits: [Float], oodLogit: Float, bioLogits: [Float], closedLogits: [[Float]]) {
        self.intentLogits = intentLogits
        self.oodLogit = oodLogit
        self.bioLogits = bioLogits
        self.closedLogits = closedLogits
    }
}

public protocol NluModelRunning: Sendable {
    func predict(inputIds: [Int32], attentionMask: [Int32]) throws -> NluModelOutputs
}
