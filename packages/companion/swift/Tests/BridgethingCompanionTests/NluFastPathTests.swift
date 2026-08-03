import Testing

@testable import BridgethingCompanion

@Suite("nlu fast path")
struct NluFastPathTests {
    @Test("bare transport commands match without slots")
    func bareTransport() {
        #expect(NluFastPath.match("play")?.intent == "PLAY")
        #expect(NluFastPath.match("pause")?.intent == "PAUSE")
        #expect(NluFastPath.match("next song")?.intent == "NEXT")
        #expect(NluFastPath.match("what's playing")?.intent == "SHOW_VIEW")
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
        #expect(hit?.intent == "PRESET_PLAY")
        #expect(hit?.slots.preset == "3")
    }

    @Test("preset rejects out-of-range and save phrasings")
    func presetBounds() {
        #expect(NluFastPath.match("play preset 7") == nil)
        #expect(NluFastPath.match("save preset 2")?.intent != "PRESET_PLAY")
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
        #expect(NluFastPath.match("What's playing?")?.intent == "SHOW_VIEW")
    }

    @Test("politeness, determiners and generic nouns do not hide the command")
    func naturalPhrasing() {
        let expected: [String: String] = [
            "Pause music now.": "PAUSE",
            "Could you pause the song?": "PAUSE",
            "Stop the song from playing.": "PAUSE",
            "Turn off music.": "PAUSE",
            "End this track.": "PAUSE",
            "Skip the track.": "NEXT",
            "Can you skip this song?": "NEXT",
            "Skip to the next track.": "NEXT",
            "Would you go to the next song please?": "NEXT",
            "Go back to previous song.": "PREVIOUS",
            "Please play the previous song.": "PREVIOUS",
            "Replay the last song.": "PREVIOUS",
            "Repeat the last song.": "PREVIOUS",
            "shuffle the tracks": "SET_SHUFFLE",
            "Put this playlist on shuffle.": "SET_SHUFFLE",
        ]
        for (utterance, intent) in expected {
            #expect(NluFastPath.match(utterance)?.intent == intent, "expected \(intent) for: \(utterance)")
        }
    }

    @Test("repeat scope survives the generic-noun strip")
    func repeatScope() {
        #expect(NluFastPath.match("Put song on repeat for me.")?.slots.repeatMode == .one)
        #expect(NluFastPath.match("Could you play this song in loop?")?.slots.repeatMode == .one)
        #expect(NluFastPath.match("Repeat this playlist indefinitely.")?.slots.repeatMode == .all)
    }

    @Test("declines anything carrying content, a scope or a second setting")
    func declinesBeyondReach() {
        for utterance in [
            "Play Pandora on Shuffle for us.",
            "Play more music like this.",
            "Skip the next two songs.",
            "Skip to track 20.",
            "Shuffle for the next five songs.",
            "Play a list of my favorite songs.",
            "Make a playlist with my most listened to tracks.",
            "Exit Spotify",
        ] {
            #expect(NluFastPath.match(utterance) == nil, "fast path must not claim: \(utterance)")
        }
    }

    @Test("a collection word blocks the transport rules the generic strip would blind")
    func collectionWordsBlockTransport() {
        for utterance in [
            "Go to the next playlist.",
            "Play my last playlist.",
            "shuffle play this playlist.",
            "next album",
            "start the next station",
        ] {
            #expect(NluFastPath.match(utterance) == nil, "fast path must not claim: \(utterance)")
        }
        #expect(NluFastPath.match("skip this song")?.intent == "NEXT")
    }

    @Test("mute folds into SET_VOLUME and whats-playing into SHOW_VIEW")
    func foldedIntents() {
        let mute = NluFastPath.match("mute")
        #expect(mute?.intent == "SET_VOLUME")
        #expect(mute?.slots.mute == true)
        let unmute = NluFastPath.match("unmute")
        #expect(unmute?.intent == "SET_VOLUME")
        #expect(unmute?.slots.mute == false)
        let whats = NluFastPath.match("what's playing")
        #expect(whats?.intent == "SHOW_VIEW")
        #expect(whats?.slots.view == .nowPlaying)
    }
}
