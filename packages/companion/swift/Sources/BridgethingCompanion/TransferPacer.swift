import Foundation

struct TransferPacer {
    static let targetDelaySeconds: Double = 0.6
    static let ackIntervalBytes: UInt64 = 16 * 1024
    static let minWindowBytes: UInt64 = 4 * TransferPacer.ackIntervalBytes
    static let maxWindowBytes: UInt64 = 16 * TransferPacer.ackIntervalBytes
    static let fragmentBytes: Int = 16 * 1024
    static let rateSampleCount: Int = 8

    private let clock: () -> Double
    private var ackedBytes: UInt64
    private var lastProgressAt: Double
    private var samples: [Double] = []

    init(startOffset: UInt64 = 0, clock: @escaping () -> Double = { ProcessInfo.processInfo.systemUptime }) {
        self.clock = clock
        ackedBytes = startOffset
        lastProgressAt = clock()
    }

    var ratePerSec: Double? { samples.max() }

    var windowBytes: UInt64 {
        guard let rate = ratePerSec else { return Self.minWindowBytes }
        let budget = UInt64((rate * Self.targetDelaySeconds).rounded())
        return min(max(budget, Self.minWindowBytes), Self.maxWindowBytes)
    }

    var fragmentBytes: Int { Self.fragmentBytes }

    mutating func observe(ackedBytes acked: UInt64) {
        guard acked > ackedBytes else { return }
        let now = clock()
        let dt = max(now - lastProgressAt, 0.001)
        samples.append(Double(acked - ackedBytes) / dt)
        if samples.count > Self.rateSampleCount { samples.removeFirst(samples.count - Self.rateSampleCount) }
        ackedBytes = acked
        lastProgressAt = now
    }
}
