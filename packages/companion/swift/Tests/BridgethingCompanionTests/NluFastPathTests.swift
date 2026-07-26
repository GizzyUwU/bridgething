import Testing

@testable import BridgethingCompanion

@Suite("nlu fast path")
struct NluFastPathTests {
    @Test("bare transport commands match without slots")
    func bareTransport() {
        #expect(NluFastPath.match("play")?.intent == "PLAY")
        #expect(NluFastPath.match("pause")?.intent == "PAUSE")
        #expect(NluFastPath.match("next song")?.intent == "NEXT")
        #expect(NluFastPath.match("what's playing")?.intent == "WHATS_PLAYING")
    }

    @Test("never fires on a command carrying content")
    func neverFiresOnContent() {
        for utterance in [
            "play some jazz",
            "play bohemian rhapsody",
            "play the new album by black country new road",
            "play my liked songs",
            "add this to my dance playlist",
            "what album is this from",
        ] {
            #expect(NluFastPath.match(utterance) == nil, "fast path must not claim: \(utterance)")
        }
    }

    @Test("preset selection captures the number")
    func presetSelection() {
        let hit = NluFastPath.match("play preset 3")
        #expect(hit?.intent == "PLAY_PRESET")
        #expect(hit?.slots.preset == "3")
    }

    @Test("preset rejects out-of-range and save phrasings")
    func presetBounds() {
        #expect(NluFastPath.match("play preset 7") == nil)
        #expect(NluFastPath.match("save preset 2")?.intent != "PLAY_PRESET")
    }

    @Test("rule order keeps overlapping repeat phrasings distinct")
    func ruleOrdering() {
        #expect(NluFastPath.match("repeat this")?.intent == "SET_REPEAT")
        #expect(NluFastPath.match("repeat this")?.slots.repeatMode == .one)
        #expect(NluFastPath.match("repeat on")?.slots.repeatMode == .all)
        #expect(NluFastPath.match("repeat off")?.slots.repeatMode == .off)
    }

    @Test("unhandled phrasings fall through instead of guessing")
    func fallsThrough() {
        #expect(NluFastPath.match("repeat one") == nil)
    }
}

@Suite("nlu fast path asr shape")
struct NluFastPathAsrShapeTests {
    @Test("matches raw recogniser output without pre-normalisation")
    func rawRecogniserOutput() {
        #expect(NluFastPath.match("Pause.")?.intent == "PAUSE")
        #expect(NluFastPath.match("Next song.")?.intent == "NEXT")
        #expect(NluFastPath.match("What's playing?")?.intent == "WHATS_PLAYING")
    }
}
