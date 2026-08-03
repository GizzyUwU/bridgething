import Foundation
import Nlu
import Testing

@testable import BridgethingNluKit

@Suite("nlu slot token mapping")
struct NluSlotMappingTests {
    func slot(_ name: String, _ value: String) -> SlotValue {
        SlotValue(name: name, value: value)
    }

    @Test("yaml tokens map onto the typed wire slots")
    func typedMapping() {
        let out = NluSlotMapping.apply([
            slot("target", "Elvis"),
            slot("target_type", "station"),
            slot("popularity_filter", "top_5"),
            slot("scope", "previous_track"),
            slot("view", "now_playing"),
            slot("repeat_mode", "off"),
            slot("speed", "1.5"),
            slot("amount", "large"),
            slot("phone_action", "unhold"),
        ])
        #expect(out.target == "Elvis")
        #expect(out.targetType == .station)
        #expect(out.popularityFilter == .top5)
        #expect(out.scope == .previousTrack)
        #expect(out.view == .nowPlaying)
        #expect(out.repeatMode == .off)
        #expect(out.speed == .onePointFive)
        #expect(out.amount == .large)
        #expect(out.phoneAction == .unhold)
    }

    @Test("python-stringified booleans and integer tokens convert")
    func scalarTokens() {
        let out = NluSlotMapping.apply([
            slot("enabled", "True"),
            slot("mute", "False"),
            slot("count", "3"),
            slot("position", "12"),
            slot("level", "80"),
            slot("seconds", "-15"),
        ])
        #expect(out.enabled == true)
        #expect(out.mute == false)
        #expect(out.count == 3)
        #expect(out.position == 12)
        #expect(out.level == 80)
        #expect(out.seconds == -15)
    }

    @Test("unknown enum tokens drop rather than guess")
    func unknownTokens() {
        let out = NluSlotMapping.apply([
            slot("target_type", "hologram"),
            slot("enabled", "maybe"),
            slot("count", "many"),
        ])
        #expect(out.targetType == nil)
        #expect(out.enabled == nil)
        #expect(out.count == nil)
    }
}
