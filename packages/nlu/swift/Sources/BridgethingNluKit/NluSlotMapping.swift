import BridgethingCompanion
import BridgethingSchema
import Foundation
import Nlu

public enum NluSlotMapping {
    public static func apply(_ slots: [SlotValue]) -> NluMutableSlots {
        var out = NluMutableSlots()
        for slot in slots {
            let v = slot.value
            switch slot.name {
            case "target": out.target = v
            case "playlist": out.playlist = v
            case "genre": out.genre = v
            case "mood": out.mood = v
            case "era": out.era = v
            case "webapp_name": out.webappName = v
            case "preset": out.preset = v
            case "target_type": out.targetType = NluTargetType(rawValue: camel(v))
            case "popularity_filter": out.popularityFilter = NluPopularityFilter(rawValue: camel(v))
            case "scope": out.scope = NluScope(rawValue: camel(v))
            case "view": out.view = NluView(rawValue: camel(v))
            case "repeat_mode": out.repeatMode = NluRepeatMode(rawValue: camel(v))
            case "speed": out.speed = NluPlaybackSpeed(rawValue: v)
            case "direction": out.direction = NluDirection(rawValue: camel(v))
            case "amount": out.amount = NluAmount(rawValue: camel(v))
            case "phone_action": out.phoneAction = NluPhoneAction(rawValue: camel(v))
            case "enabled": out.enabled = bool(v)
            case "mute": out.mute = bool(v)
            case "count": out.count = UInt32(v)
            case "position": out.position = UInt32(v)
            case "level": out.level = UInt32(v)
            case "seconds": out.seconds = Int32(v)
            default: break
            }
        }
        return out
    }

    static func camel(_ token: String) -> String {
        let parts = token.split(separator: "_")
        guard let first = parts.first else { return token }
        return String(first) + parts.dropFirst().map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined()
    }

    static func bool(_ token: String) -> Bool? {
        switch token.lowercased() {
        case "true": return true
        case "false": return false
        default: return nil
        }
    }
}
