import BridgethingCompanionCore
import Foundation

public final class FoundationHostEnvironment: HostEnvironment, @unchecked Sendable {
    public init() {}

    public func clock() -> HostClock {
        let now = Date()
        let tz = TimeZone.current
        let total = tz.secondsFromGMT(for: now)
        let dst = Int(tz.daylightSavingTimeOffset(for: now))
        return HostClock(
            tzIana: tz.identifier,
            locale: Locale.current.identifier.replacingOccurrences(of: "_", with: "-"),
            unixSeconds: UInt64(max(now.timeIntervalSince1970, 0)),
            utcOffsetMinutes: Int16(clamping: (total - dst) / 60),
            dstOffsetMinutes: Int8(clamping: dst / 60)
        )
    }
}
