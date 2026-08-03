import BridgethingSchema
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

    static let generic: Set<String> = [
        "the", "this", "that", "these", "those", "a", "an", "my", "our", "its", "it",
        "current", "currently", "some", "song", "songs", "track", "tracks", "music",
        "playback", "tune", "tunes", "playlist", "playlists", "can", "could", "would",
        "will", "shall", "you", "your", "i", "we", "us", "let", "lets", "want", "wanna",
        "need", "gotta", "must", "should", "may", "might", "now", "immediately", "for",
        "me", "of", "already", "right",
    ]

    public static func match(_ transcript: String) -> Hit? {
        let (raw, tokens) = normalize(transcript)
        guard !tokens.isEmpty else { return nil }
        let core = tokens.filter { !generic.contains($0) }.joined(separator: " ")
        for rule in rules {
            if let hit = rule(tokens, raw, core) { return hit }
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

    static func coreIsOnly(_ core: String, target: Set<String>, leads: Set<String> = []) -> Bool {
        let tokens = core.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
        guard tokens.contains(where: { target.contains($0) }) else { return false }
        return tokens.allSatisfy { target.contains($0) || leads.contains($0) }
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

    static func rulePlayPreset(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
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
        return Hit(intent: "PRESET_PLAY", slots: NluMutableSlots(preset: String(n)))
    }

    static func ruleSaveToPreset(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        guard tokens.contains("preset"), tokens.contains("save") || tokens.contains("store") else { return nil }
        guard let captured = captureGroup(raw, presetRe), let n = parseInt(captured), 1...4 ~= n else { return nil }
        return Hit(intent: "PRESET_SAVE", slots: NluMutableSlots(preset: String(n)))
    }

    static func ruleVolumeAbsolute(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        guard tokens.contains("volume") || tokens.contains("level") else { return nil }
        let patterns = [
            "(?:set|put)\\s+(?:the\\s+)?volume\\s+(?:to|at)\\s+([\\w\\s-]+?)(?:\\s+percent|\\s*$|\\s+please\\b)",
            "\\bvolume\\s+([\\w\\s-]+?)\\s+percent\\b",
            "\\bvolume\\s+(?:to|at)\\s+([\\w\\s-]+?)(?:\\s*$|\\s+percent)",
            "volume\\s+(?:to|at)?\\s*(\\d+|[a-z]+)\\s*(?:percent)?\\s*$",
        ]
        for pat in patterns {
            if let captured = captureGroup(raw, pat), let n = parseInt(captured), 1...100 ~= n {
                return Hit(intent: "SET_VOLUME", slots: NluMutableSlots(level: UInt32(n)))
            }
        }
        return nil
    }

    static let speedRules: [(NluPlaybackSpeed, [String])] = [
        (.onePointTwo, [
            "\\bone\\s+point\\s+two(?:\\s+(?:speed|x|times))?\\b",
            "\\b1\\.2\\s*x?\\b",
            "\\b(?:play\\s+it\\s+|speed\\s+)faster\\b",
            "\\bspeed\\s+(?:it\\s+)?up\\b",
            "\\ba\\s+little\\s+faster\\b",
            "\\bfaster\\s+a\\s+little\\b",
        ]),
        (.onePointFive, [
            "\\bone\\s+(?:and\\s+a\\s+)?half(?:\\s+speed)?\\b",
            "\\b1\\.5\\s*x?\\b",
            "\\bone\\s+point\\s+five\\b",
        ]),
        (.one, [
            "\\bnormal\\s+speed\\b",
            "\\b(?:back\\s+to\\s+|reset\\s+to\\s+)?(?:1\\s*x|one\\s+x|original\\s+speed)\\b",
            "\\b(?:play\\s+(?:it\\s+)?(?:at\\s+)?|at\\s+)normal(?:\\s+speed)?\\b",
        ]),
        (.two, [
            "\\bdouble\\s+speed\\b",
            "\\b2\\s*x\\b",
            "\\btwo\\s+x\\b",
            "\\btwo\\s+times(?:\\s+speed)?\\b",
        ]),
    ]

    static func rulePlaybackSpeed(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        let anchors = ["speed", "faster", "slower", "normal", "double", "2x", "1.5", "1.2",
                       "two x", "two times", "one point", "half"]
        guard anchors.contains(where: { raw.contains($0) }) else { return nil }
        for (speed, patterns) in speedRules {
            for p in patterns where contains(raw, p) {
                return Hit(intent: "SET_PLAYBACK_SPEED", slots: NluMutableSlots(speed: speed))
            }
        }
        return nil
    }

    static func ruleSeek(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        let has15 = contains(raw, "\\b(?:15|fifteen)\\b")
        if has15 && contains(raw, "\\b(?:rewind|go\\s+back|back|skip\\s+back)\\b") {
            return Hit(intent: "SEEK_RELATIVE", slots: NluMutableSlots(seconds: -15))
        }
        if contains(raw, "^back\\s+fifteen\\s*$") {
            return Hit(intent: "SEEK_RELATIVE", slots: NluMutableSlots(seconds: -15))
        }
        if has15 && contains(raw, "\\b(?:fast\\s+forward|forward|skip\\s+(?:ahead|forward))\\b") {
            return Hit(intent: "SEEK_RELATIVE", slots: NluMutableSlots(seconds: 15))
        }
        if contains(raw, "\\bjump\\s+ahead\\b") && contains(raw, "\\bjump\\s+ahead(?:\\s+(?:fifteen|15))?\\s*(?:seconds?)?\\s*$") {
            return Hit(intent: "SEEK_RELATIVE", slots: NluMutableSlots(seconds: 15))
        }
        if contains(raw, "^forward\\s+fifteen\\s*$") {
            return Hit(intent: "SEEK_RELATIVE", slots: NluMutableSlots(seconds: 15))
        }
        return nil
    }

    static let collectionRe = "\\b(?:playlist|album|queue|everything|all)\\b"

    static let namedCollectionRe = "\\b(?:playlists?|albums?|stations?|podcasts?|artists?)\\b"

    static func ruleRepeat(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        guard tokens.contains("repeat") || tokens.contains("loop") || tokens.contains("looped")
            || contains(raw, "\\bover\\s+and\\s+over\\b") else { return nil }
        guard !raw.contains("shuffl") else { return nil }

        if contains(raw, "\\brepeat\\s+off\\b") || contains(raw, "\\bstop\\s+repeat(?:ing)?\\b")
            || contains(raw, "\\bturn\\s+(?:off|of)\\s+repeat\\b") || contains(raw, "\\bdisable\\s+repeat\\b")
            || contains(raw, "\\bstop\\s+looping\\b") {
            return Hit(intent: "SET_REPEAT", slots: NluMutableSlots(repeatMode: .off))
        }

        let wholeCollection = contains(raw, collectionRe)
        if !wholeCollection {
            if contains(raw, "\\b(?:repeat|loop)\\s+(?:this(?:\\s+(?:song|track|one))?|current(?:\\s+(?:song|track))?|it)\\b")
                || contains(raw, "\\bon\\s+repeat\\b") || contains(raw, "\\b(?:in|on)\\s+(?:a\\s+)?(?:repeat\\s+)?loop\\b")
                || contains(raw, "\\bbe\\s+looped\\b") || contains(raw, "\\bloop\\s+(?:this|that|it)\\b")
                || contains(raw, "\\bover\\s+and\\s+over\\b") {
                return Hit(intent: "SET_REPEAT", slots: NluMutableSlots(repeatMode: .one))
            }
        }

        if wholeCollection && contains(raw, "\\b(?:repeat|loop)\\b") {
            return Hit(intent: "SET_REPEAT", slots: NluMutableSlots(repeatMode: .all))
        }
        if contains(raw, "\\brepeat(?:\\s+on)?\\s*$") || ["repeat", "loop", "repeat on", "loop on"].contains(raw)
            || contains(raw, "\\bturn\\s+on\\s+repeat\\b") || contains(raw, "\\benable\\s+repeat\\b") {
            return Hit(intent: "SET_REPEAT", slots: NluMutableSlots(repeatMode: .all))
        }
        return nil
    }

    static func ruleShuffle(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        guard tokens.contains("shuffle") || tokens.contains("shuffling") || tokens.contains("mix")
            || tokens.contains("randomize") else { return nil }
        if (tokens.contains("play") || tokens.contains("start")) && contains(raw, namedCollectionRe) { return nil }
        if contains(raw, "\\bshuffle\\s+off\\b") || contains(raw, "\\bstop\\s+shuffling\\b")
            || contains(raw, "\\bturn\\s+off\\s+shuffle\\b") || contains(raw, "\\bdisable\\s+shuffle\\b") {
            return Hit(intent: "SET_SHUFFLE", slots: NluMutableSlots(enabled: false))
        }
        if coreIsOnly(core, target: ["shuffle", "shuffling", "mix"], leads: ["on", "up", "turn", "enable", "start", "play", "and", "repeat", "put", "mode", "needs", "randomize"]) {
            return Hit(intent: "SET_SHUFFLE", slots: NluMutableSlots(enabled: true))
        }
        return nil
    }

    static func ruleWhatsPlaying(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
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
            return Hit(intent: "SHOW_VIEW", slots: NluMutableSlots(view: .nowPlaying))
        }
        return nil
    }

    static func ruleUnmute(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        if contains(raw, "\\bunmute\\b") { return Hit(intent: "SET_VOLUME", slots: NluMutableSlots(mute: false)) }
        if contains(raw, "\\bturn\\s+(?:the\\s+)?(?:sound|audio|volume)\\s+back\\s+on\\b") {
            return Hit(intent: "SET_VOLUME", slots: NluMutableSlots(mute: false))
        }
        return nil
    }

    static func ruleMute(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        if contains(raw, "^mute(?:\\s+(?:it|the\\s+(?:audio|sound|volume|music)))?\\s*$") {
            return Hit(intent: "SET_VOLUME", slots: NluMutableSlots(mute: true))
        }
        if contains(raw, "\\bturn\\s+(?:off|down\\s+to\\s+zero)\\s+(?:the\\s+)?(?:sound|audio|volume)\\b") {
            return Hit(intent: "SET_VOLUME", slots: NluMutableSlots(mute: true))
        }
        return nil
    }

    static func amountModifier(_ raw: String) -> NluAmount {
        if contains(raw, "\\ba\\s+(?:little|bit|tiny\\s+bit|touch)\\b") { return .small }
        if contains(raw, "\\ba\\s+lot\\b|\\bway\\b|\\bmuch\\s+(?:louder|higher|quieter|lower)\\b") { return .large }
        return .medium
    }

    static func ruleVolumeUp(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        let patterns = [
            "\\bvolume\\s+up\\b",
            "^louder\\s*$",
            "\\bturn\\s+(?:it|the\\s+(?:volume|music))?\\s*up\\b",
            "\\bturn\\s+up\\s+(?:the\\s+)?volume\\b",
            "\\bcrank\\s+(?:it\\s+)?up\\b",
            "^make\\s+(?:it\\s+)?louder\\s*$",
        ]
        for p in patterns where contains(raw, p) {
            return Hit(intent: "SET_VOLUME", slots: NluMutableSlots(direction: .up, amount: amountModifier(raw)))
        }
        return nil
    }

    static func ruleVolumeDown(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        if contains(raw, "\\bvolume\\s+down\\b") || ["quieter", "softer"].contains(raw)
            || contains(raw, "\\bturn\\s+(?:it|the\\s+(?:volume|music))?\\s*down\\b")
            || contains(raw, "\\bturn\\s+down\\s+(?:the\\s+)?volume\\b")
            || contains(raw, "^make\\s+(?:it\\s+)?(?:quieter|softer)\\s*$") {
            return Hit(intent: "SET_VOLUME", slots: NluMutableSlots(direction: .down, amount: amountModifier(raw)))
        }
        return nil
    }

    static func rulePlayResume(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        if coreIsOnly(core, target: ["resume"], leads: ["playing", "play"]) {
            return Hit(intent: "PLAY", slots: NluMutableSlots())
        }
        if ["keep playing", "keep going"].contains(core) {
            return Hit(intent: "PLAY", slots: NluMutableSlots())
        }
        return nil
    }

    static func rulePause(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        if coreIsOnly(core, target: ["pause"], leads: ["playing"]) {
            return Hit(intent: "PAUSE", slots: NluMutableSlots())
        }
        return nil
    }

    static func rulePauseStop(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        if tokens.contains("repeat") || raw.contains("shuffl") { return nil }
        if coreIsOnly(core, target: ["stop", "end"], leads: ["playing", "play", "from"]) {
            return Hit(intent: "PAUSE", slots: NluMutableSlots())
        }
        if coreIsOnly(core, target: ["off"], leads: ["turn", "playing", "play"]) {
            return Hit(intent: "PAUSE", slots: NluMutableSlots())
        }
        return nil
    }

    static func ruleNext(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        guard !core.contains("back"), !contains(raw, namedCollectionRe) else { return nil }
        if coreIsOnly(core, target: ["next", "skip"], leads: ["play", "go", "hear", "listen", "to", "one", "ahead", "forward"]) {
            return Hit(intent: "NEXT", slots: NluMutableSlots())
        }
        return nil
    }

    static let previousLeads: Set<String> = [
        "play", "go", "hear", "listen", "to", "back", "one", "again", "more", "time",
        "start", "from", "beginning", "over",
    ]

    static func rulePrevious(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        guard !contains(raw, namedCollectionRe) else { return nil }
        if coreIsOnly(core, target: ["previous", "replay"], leads: previousLeads.union(["last"])) {
            return Hit(intent: "PREVIOUS", slots: NluMutableSlots())
        }
        if coreIsOnly(core, target: ["last"], leads: previousLeads.union(["repeat", "replay"])),
            contains(core, "\\b(?:play|go|hear|listen|back|repeat|replay|start)\\b")
        {
            return Hit(intent: "PREVIOUS", slots: NluMutableSlots())
        }
        if coreIsOnly(core, target: ["back"], leads: ["go", "one", "to"]) {
            return Hit(intent: "PREVIOUS", slots: NluMutableSlots())
        }
        return nil
    }

    static func rulePlayBare(_ tokens: [String], _ raw: String, _ core: String) -> Hit? {
        if coreIsOnly(core, target: ["play", "start"], leads: ["playing", "something", "go", "on"]) {
            return Hit(intent: "PLAY", slots: NluMutableSlots())
        }
        if core == "go" { return Hit(intent: "PLAY", slots: NluMutableSlots()) }
        return nil
    }

    static let rules: [@Sendable ([String], String, String) -> Hit?] = [
        ruleSaveToPreset,
        rulePlayPreset,
        ruleVolumeAbsolute,
        rulePlaybackSpeed,
        ruleSeek,
        ruleRepeat,
        ruleShuffle,
        ruleWhatsPlaying,
        ruleUnmute,
        ruleMute,
        ruleVolumeUp,
        ruleVolumeDown,
        rulePlayResume,
        rulePause,
        rulePauseStop,
        ruleNext,
        rulePrevious,
        rulePlayBare,
    ]
}
