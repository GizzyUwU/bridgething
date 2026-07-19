import Foundation

struct TransferPacer {
    static let targetDelaySeconds: Double = 0.6
    static let minWindowBytes: UInt64 = 4 * 1024
    static let maxWindowBytes: UInt64 = 64 * 1024
    static let largeFragmentBytes: Int = 16 * 1024
    static let smallFragmentBytes: Int = 4 * 1024
    static let fragmentLadderBytes: UInt64 = 32 * 1024
    private static let ewmaAlpha: Double = 0.3

    private let clock: () -> Double
    private var ackedBytes: UInt64
    private var lastProgressAt: Double
    private var ratePerSec: Double?

    init(startOffset: UInt64 = 0, clock: @escaping () -> Double = { ProcessInfo.processInfo.systemUptime }) {
        self.clock = clock
        self.ackedBytes = startOffset
        self.lastProgressAt = clock()
    }

    var windowBytes: UInt64 {
        guard let rate = ratePerSec else { return UInt64(Self.largeFragmentBytes) }
        let bdp = UInt64((rate * Self.targetDelaySeconds).rounded())
        return min(max(bdp, Self.minWindowBytes), Self.maxWindowBytes)
    }

    var fragmentBytes: Int {
        guard ratePerSec != nil else { return Self.largeFragmentBytes }
        return windowBytes >= Self.fragmentLadderBytes ? Self.largeFragmentBytes : Self.smallFragmentBytes
    }

    mutating func observe(ackedBytes acked: UInt64) {
        guard acked > ackedBytes else { return }
        let now = clock()
        let dt = max(now - lastProgressAt, 0.001)
        let instantaneous = Double(acked - ackedBytes) / dt
        if dt > 2 * Self.targetDelaySeconds {
            ratePerSec = instantaneous
        } else {
            ratePerSec = ratePerSec.map { $0 + Self.ewmaAlpha * (instantaneous - $0) } ?? instantaneous
        }
        ackedBytes = acked
        lastProgressAt = now
    }
}
