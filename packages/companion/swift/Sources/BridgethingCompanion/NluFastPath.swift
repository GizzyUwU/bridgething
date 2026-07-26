import Foundation

public enum NluFastPath {
    static let fillers: Set<String> = [
        "uh", "uhh", "uhhh", "uhhhh", "um", "umm", "hmm", "hmmm",
        "er", "eh", "ah", "oh", "well", "so", "like", "yeah", "yep",
        "yes", "ok", "okay", "hey", "please", "thanks", "thank",
        "mean", "wait",
    ]

    static let wordToNumber: [String: Int] = [
        "zero": 0, "one": 1, "two": 2, "three": 3, "four": 4, "five": 5,
        "six": 6, "seven": 7, "eight": 8, "nine": 9, "ten": 10,
        "eleven": 11, "twelve": 12, "thirteen": 13, "fourteen": 14, "fifteen": 15,
        "sixteen": 16, "seventeen": 17, "eighteen": 18, "nineteen": 19,
        "twenty": 20, "thirty": 30, "forty": 40, "fifty": 50, "sixty": 60,
        "seventy": 70, "eighty": 80, "ninety": 90, "hundred": 100,
    ]

    public struct Hit: Equatable {
        public let intent: String
        public let slots: NluMutableSlots
    }

    public static func match(_ transcript: String) -> Hit? {
        let (raw, tokens) = normalize(transcript)
        guard !tokens.isEmpty else { return nil }
        for rule in rules {
            if let hit = rule(tokens, raw) { return hit }
        }
        return nil
    }

    static func normalize(_ transcript: String) -> (String, [String]) {
        var lowered = transcript.lowercased()
        let punctRe = try! NSRegularExpression(pattern: "[^\\w\\s']")
        let range = NSRange(lowered.startIndex..., in: lowered)
        lowered = punctRe.stringByReplacingMatches(in: lowered, range: range, withTemplate: " ")
        let allTokens = lowered.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
        let tokens = allTokens.filter { !fillers.contains($0) }
        return (tokens.joined(separator: " "), tokens)
    }

    static func parseInt(_ s: String) -> Int? {
        let cleaned = s
            .replacingOccurrences(of: "-", with: " ")
            .replacingOccurrences(of: "percent", with: "")
            .trimmingCharacters(in: .whitespaces)
        guard !cleaned.isEmpty else { return nil }
        if let n = Int(cleaned), 0...100 ~= n { return n }
        let parts = cleaned.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
        var total = 0
        for p in parts {
            guard let v = wordToNumber[p] else { return nil }
            if v == 100 {
                total = max(total, 1) * 100
            } else {
                total += v
            }
        }
        return 0...100 ~= total ? total : nil
    }

    static func regex(_ pattern: String) -> NSRegularExpression {
        try! NSRegularExpression(pattern: pattern)
    }

    static func contains(_ text: String, _ pattern: String) -> Bool {
        let re = regex(pattern)
        return re.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) != nil
    }

    static func captureGroup(_ text: String, _ pattern: String) -> String? {
        let re = regex(pattern)
        let range = NSRange(text.startIndex..., in: text)
        guard let m = re.firstMatch(in: text, range: range), m.numberOfRanges >= 2 else { return nil }
        guard let r = Range(m.range(at: 1), in: text) else { return nil }
        return String(text[r])
    }

    static let presetRe = "preset\\s+(\\w+)"

    static func rulePlayPreset(_ tokens: [String], _ raw: String) -> Hit? {
        guard tokens.contains("preset") else { return nil }
        let leadOk: Bool = {
            if let first = tokens.first, ["play", "load", "switch", "go", "select"].contains(first) { return true }
            if Array(tokens.prefix(2)) == ["go", "to"] { return true }
            return false
        }()
        guard leadOk else { return nil }
        if tokens.contains("save") || tokens.contains("store") { return nil }
        guard let captured = captureGroup(raw, presetRe), let n = parseInt(captured), 1...4 ~= n else { return nil }
        if let after = raw.range(of: "preset") {
            let trailing = raw[after.upperBound...].trimmingCharacters(in: .whitespaces).split(separator: " ")
            if trailing.count > 2 { return nil }
        }
        return Hit(intent: "PLAY_PRESET", slots: NluMutableSlots(preset: String(n)))
    }

    static func ruleSaveToPreset(_ tokens: [String], _ raw: String) -> Hit? {
        guard tokens.contains("preset"), tokens.contains("save") || tokens.contains("store") else { return nil }
        guard let captured = captureGroup(raw, presetRe), let n = parseInt(captured), 1...4 ~= n else { return nil }
        return Hit(intent: "SAVE_TO_PRESET", slots: NluMutableSlots(preset: String(n)))
    }

    static func ruleVolumeAbsolute(_ tokens: [String], _ raw: String) -> Hit? {
        guard tokens.contains("volume") || tokens.contains("level") else { return nil }
        let patterns = [
            "(?:set|put)\\s+(?:the\\s+)?volume\\s+(?:to|at)\\s+([\\w\\s-]+?)(?:\\s+percent|\\s*$|\\s+please\\b)",
            "\\bvolume\\s+([\\w\\s-]+?)\\s+percent\\b",
            "\\bvolume\\s+(?:to|at)\\s+([\\w\\s-]+?)(?:\\s*$|\\s+percent)",
            "volume\\s+(?:to|at)?\\s*(\\d+|[a-z]+)\\s*(?:percent)?\\s*$",
        ]
        for pat in patterns {
            if let captured = captureGroup(raw, pat), let n = parseInt(captured), 1...100 ~= n {
                return Hit(intent: "VOLUME_ABSOLUTE", slots: NluMutableSlots(level: UInt32(n)))
            }
        }
        return nil
    }

    static let speedRules: [(String, [String])] = [
        ("SET_PLAYBACK_SPEED_1POINT2X", [
            "\\bone\\s+point\\s+two(?:\\s+(?:speed|x|times))?\\b",
            "\\b1\\.2\\s*x?\\b",
            "\\b(?:play\\s+it\\s+|speed\\s+)faster\\b",
            "\\bspeed\\s+(?:it\\s+)?up\\b",
            "\\ba\\s+little\\s+faster\\b",
            "\\bfaster\\s+a\\s+little\\b",
        ]),
        ("SET_PLAYBACK_SPEED_1POINT5X", [
            "\\bone\\s+(?:and\\s+a\\s+)?half(?:\\s+speed)?\\b",
            "\\b1\\.5\\s*x?\\b",
            "\\bone\\s+point\\s+five\\b",
        ]),
        ("SET_PLAYBACK_SPEED_1X", [
            "\\bnormal\\s+speed\\b",
            "\\b(?:back\\s+to\\s+|reset\\s+to\\s+)?(?:1\\s*x|one\\s+x|original\\s+speed)\\b",
            "\\b(?:play\\s+(?:it\\s+)?(?:at\\s+)?|at\\s+)normal(?:\\s+speed)?\\b",
        ]),
        ("SET_PLAYBACK_SPEED_2X", [
            "\\bdouble\\s+speed\\b",
            "\\b2\\s*x\\b",
            "\\btwo\\s+x\\b",
            "\\btwo\\s+times(?:\\s+speed)?\\b",
        ]),
    ]

    static func rulePlaybackSpeed(_ tokens: [String], _ raw: String) -> Hit? {
        let anchors = ["speed", "faster", "slower", "normal", "double", "2x", "1.5", "1.2",
                       "two x", "two times", "one point", "half"]
        guard anchors.contains(where: { raw.contains($0) }) else { return nil }
        for (intent, patterns) in speedRules {
            for p in patterns where contains(raw, p) {
                return Hit(intent: intent, slots: NluMutableSlots())
            }
        }
        return nil
    }

    static func ruleSeek(_ tokens: [String], _ raw: String) -> Hit? {
        let has15 = contains(raw, "\\b(?:15|fifteen)\\b")
        if has15 && contains(raw, "\\b(?:rewind|go\\s+back|back|skip\\s+back)\\b") {
            return Hit(intent: "SEEK_BACK_15_SECONDS", slots: NluMutableSlots())
        }
        if contains(raw, "^back\\s+fifteen\\s*$") {
            return Hit(intent: "SEEK_BACK_15_SECONDS", slots: NluMutableSlots())
        }
        if has15 && contains(raw, "\\b(?:fast\\s+forward|forward|skip\\s+(?:ahead|forward))\\b") {
            return Hit(intent: "SEEK_FORWARD_15_SECONDS", slots: NluMutableSlots())
        }
        if contains(raw, "\\bjump\\s+ahead\\b") && contains(raw, "\\bjump\\s+ahead(?:\\s+(?:fifteen|15))?\\s*(?:seconds?)?\\s*$") {
            return Hit(intent: "SEEK_FORWARD_15_SECONDS", slots: NluMutableSlots())
        }
        if contains(raw, "^forward\\s+fifteen\\s*$") {
            return Hit(intent: "SEEK_FORWARD_15_SECONDS", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleRepeat(_ tokens: [String], _ raw: String) -> Hit? {
        guard tokens.contains("repeat") || tokens.contains("loop") else { return nil }
        if contains(raw, "\\b(?:repeat|loop)\\s+(?:this(?:\\s+(?:song|track|one))?|current(?:\\s+(?:song|track))?|it)\\b") {
            return Hit(intent: "REPEAT_ONE", slots: NluMutableSlots())
        }
        if contains(raw, "\\b(?:play\\s+)?this\\s+(?:song\\s+)?on\\s+repeat\\b") {
            return Hit(intent: "REPEAT_ONE", slots: NluMutableSlots())
        }
        if contains(raw, "\\brepeat\\s+off\\b") || contains(raw, "\\bstop\\s+repeat(?:ing)?\\b")
            || contains(raw, "\\bturn\\s+(?:off|of)\\s+repeat\\b") || contains(raw, "\\bdisable\\s+repeat\\b") {
            return Hit(intent: "REPEAT_OFF", slots: NluMutableSlots())
        }
        if contains(raw, "\\brepeat(?:\\s+on)?\\s*$") || ["repeat", "loop", "repeat on", "loop on"].contains(raw)
            || contains(raw, "\\bturn\\s+on\\s+repeat\\b") || contains(raw, "\\benable\\s+repeat\\b") {
            return Hit(intent: "REPEAT_ON", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleShuffle(_ tokens: [String], _ raw: String) -> Hit? {
        guard tokens.contains("shuffle") || tokens.contains("shuffling") else { return nil }
        if contains(raw, "\\bshuffle\\s+off\\b") || contains(raw, "\\bstop\\s+shuffling\\b")
            || contains(raw, "\\bturn\\s+off\\s+shuffle\\b") || contains(raw, "\\bdisable\\s+shuffle\\b") {
            return Hit(intent: "SHUFFLE_OFF", slots: NluMutableSlots())
        }
        if ["shuffle", "shuffle on"].contains(raw)
            || contains(raw, "\\bshuffle\\s+(?:on|please)?\\s*$")
            || contains(raw, "\\bturn\\s+on\\s+shuffle\\b")
            || contains(raw, "\\benable\\s+shuffle\\b") {
            return Hit(intent: "SHUFFLE_ON", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleWhatsPlaying(_ tokens: [String], _ raw: String) -> Hit? {
        guard tokens.count <= 8 else { return nil }
        let patterns = [
            "^what'?s\\s+playing\\s*$",
            "^what'?s\\s+this(?:\\s+song)?\\s*$",
            "^what\\s+is\\s+this(?:\\s+song)?\\s*$",
            "^who'?s\\s+(?:this|playing)\\s*$",
            "^who\\s+is\\s+(?:this|playing)\\s*$",
            "\\bname\\s+of\\s+(?:the\\s+|this\\s+)?(?:song|track|artist)\\b",
            "^what\\s+song\\s+is\\s+this\\s*$",
        ]
        for p in patterns where contains(raw, p) {
            return Hit(intent: "WHATS_PLAYING", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleTransferPlayback(_ tokens: [String], _ raw: String) -> Hit? {
        if contains(raw, "\\btransfer\\s+playback\\b") { return Hit(intent: "TRANSFER_PLAYBACK", slots: NluMutableSlots()) }
        if contains(raw, "\\b(?:move|send|cast)\\s+(?:this|the\\s+(?:music|audio|playback))\\s+to\\s+(?:my|the)\\b") {
            return Hit(intent: "TRANSFER_PLAYBACK", slots: NluMutableSlots())
        }
        if contains(raw, "\\bplay\\s+on\\s+(?:my\\s+)?(?:speaker|tv|chromecast|sonos|stereo|receiver)\\b") {
            return Hit(intent: "TRANSFER_PLAYBACK", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleUnmute(_ tokens: [String], _ raw: String) -> Hit? {
        if contains(raw, "\\bunmute\\b") { return Hit(intent: "UNMUTE", slots: NluMutableSlots()) }
        if contains(raw, "\\bturn\\s+(?:the\\s+)?(?:sound|audio|volume)\\s+back\\s+on\\b") {
            return Hit(intent: "UNMUTE", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleMute(_ tokens: [String], _ raw: String) -> Hit? {
        if contains(raw, "^mute(?:\\s+(?:it|the\\s+(?:audio|sound|volume|music)))?\\s*$") {
            return Hit(intent: "MUTE", slots: NluMutableSlots())
        }
        if contains(raw, "\\bturn\\s+(?:off|down\\s+to\\s+zero)\\s+(?:the\\s+)?(?:sound|audio|volume)\\b") {
            return Hit(intent: "MUTE", slots: NluMutableSlots())
        }
        return nil
    }

    static func amountModifier(_ raw: String) -> String {
        if contains(raw, "\\ba\\s+(?:little|bit|tiny\\s+bit|touch)\\b") { return "small" }
        if contains(raw, "\\ba\\s+lot\\b|\\bway\\b|\\bmuch\\s+(?:louder|higher|quieter|lower)\\b") { return "large" }
        return "medium"
    }

    static func ruleVolumeUp(_ tokens: [String], _ raw: String) -> Hit? {
        let patterns = [
            "\\bvolume\\s+up\\b",
            "^louder\\s*$",
            "\\bturn\\s+(?:it|the\\s+(?:volume|music))?\\s*up\\b",
            "\\bturn\\s+up\\s+(?:the\\s+)?volume\\b",
            "\\bcrank\\s+(?:it\\s+)?up\\b",
            "^make\\s+(?:it\\s+)?louder\\s*$",
        ]
        for p in patterns where contains(raw, p) {
            return Hit(intent: "VOLUME_UP", slots: NluMutableSlots(amount: amountModifier(raw)))
        }
        return nil
    }

    static func ruleVolumeDown(_ tokens: [String], _ raw: String) -> Hit? {
        if contains(raw, "\\bvolume\\s+down\\b") || ["quieter", "softer"].contains(raw)
            || contains(raw, "\\bturn\\s+(?:it|the\\s+(?:volume|music))?\\s*down\\b")
            || contains(raw, "\\bturn\\s+down\\s+(?:the\\s+)?volume\\b")
            || contains(raw, "^make\\s+(?:it\\s+)?(?:quieter|softer)\\s*$") {
            return Hit(intent: "VOLUME_DOWN", slots: NluMutableSlots(amount: amountModifier(raw)))
        }
        return nil
    }

    static func ruleResume(_ tokens: [String], _ raw: String) -> Hit? {
        let exact: Set<String> = ["resume", "keep playing", "resume the music", "resume music", "resume playing"]
        if exact.contains(raw) { return Hit(intent: "RESUME", slots: NluMutableSlots()) }
        if contains(raw, "^resume(?:\\s+(?:the\\s+)?(?:music|song|track|playback|playing))?\\s*$") {
            return Hit(intent: "RESUME", slots: NluMutableSlots())
        }
        if contains(raw, "^keep\\s+(?:playing|going)\\s*$") {
            return Hit(intent: "RESUME", slots: NluMutableSlots())
        }
        return nil
    }

    static func rulePause(_ tokens: [String], _ raw: String) -> Hit? {
        if contains(raw, "^pause(?:\\s+(?:it|the\\s+(?:music|song|track|playback)))?\\s*$") {
            return Hit(intent: "PAUSE", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleStop(_ tokens: [String], _ raw: String) -> Hit? {
        if tokens.contains("repeat") || raw.contains("shuffl") { return nil }
        if contains(raw, "^stop(?:\\s+(?:it|playing|the\\s+(?:music|song|track|playback)))?\\s*$") {
            return Hit(intent: "STOP", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleNext(_ tokens: [String], _ raw: String) -> Hit? {
        if contains(raw, "^next(?:\\s+(?:track|song|one))?\\s*$") { return Hit(intent: "NEXT", slots: NluMutableSlots()) }
        if contains(raw, "^skip(?:\\s+(?:this|track|song|to\\s+next))?\\s*$") {
            return Hit(intent: "NEXT", slots: NluMutableSlots())
        }
        return nil
    }

    static func rulePrevious(_ tokens: [String], _ raw: String) -> Hit? {
        if contains(raw, "^previous(?:\\s+(?:track|song|one))?\\s*$") {
            return Hit(intent: "PREVIOUS", slots: NluMutableSlots())
        }
        if contains(raw, "^(?:go\\s+)?back(?:\\s+(?:one|a\\s+track|to\\s+(?:last|previous)))?\\s*$") {
            return Hit(intent: "PREVIOUS", slots: NluMutableSlots())
        }
        return nil
    }

    static func rulePlayBare(_ tokens: [String], _ raw: String) -> Hit? {
        let exact: Set<String> = [
            "play", "play music", "play the music", "play it", "start", "start playing",
            "start the music", "play something", "go", "play more",
        ]
        if exact.contains(raw) { return Hit(intent: "PLAY", slots: NluMutableSlots()) }
        if contains(raw, "^play(?:\\s+(?:the\\s+)?music)?\\s*$") {
            return Hit(intent: "PLAY", slots: NluMutableSlots())
        }
        return nil
    }

    static let rules: [@Sendable ([String], String) -> Hit?] = [
        ruleSaveToPreset,
        rulePlayPreset,
        ruleVolumeAbsolute,
        rulePlaybackSpeed,
        ruleSeek,
        ruleRepeat,
        ruleShuffle,
        ruleWhatsPlaying,
        ruleTransferPlayback,
        ruleUnmute,
        ruleMute,
        ruleVolumeUp,
        ruleVolumeDown,
        ruleResume,
        rulePause,
        ruleStop,
        ruleNext,
        rulePrevious,
        rulePlayBare,
    ]
}
