import Foundation

/// Filler-strip helper for WEBAPP_INTENT raw_query slots.
///
/// Conservative on heavy disfluencies: removes single-word fillers,
/// compound hedges, RESTART-verb prefixes, and the heavy-suffix tail.
/// Does NOT resolve self-corrections (`turn off no wait turn on`); that
/// needs a semantic pass.
public enum NluFillerStrip {
    static let singleFillers: Set<String> = [
        "uh", "uhh", "uhhh", "um", "umm", "uhm",
        "er", "err", "ah", "ahh", "hmm", "hmmm", "mm", "mmm",
        "like", "well", "so", "yeah", "okay", "ok", "right",
        "actually", "basically", "literally", "honestly",
    ]

    static let compoundFillers: [String] = [
        "you know what",
        "you know",
        "i mean like",
        "i mean",
        "kind of",
        "sort of",
        "i guess",
        "i think",
        "let me think",
        "hold on",
        "wait wait",
    ]

    static let restartVerbPrefix: NSRegularExpression = {
        let pattern = "^(?:play|queue\\s+up|put\\s+on|show\\s+me|save|find|search\\s+for|follow|skip)\\s+(?:uh|um|er)\\b\\s*(?:i\\s+mean\\b\\s*)?"
        return try! NSRegularExpression(pattern: pattern, options: [.caseInsensitive])
    }()

    static let heavySuffix: NSRegularExpression = {
        let pattern = "\\s+(?:yeah\\s+uh\\s+that\\s+one|yeah\\s+that(?:\\s+one)?|that\\s+one|or\\s+whatever)\\s*$"
        return try! NSRegularExpression(pattern: pattern, options: [.caseInsensitive])
    }()

    static let whitespace: NSRegularExpression = {
        try! NSRegularExpression(pattern: "\\s+")
    }()

    static let punctuationEdge: NSRegularExpression = {
        try! NSRegularExpression(pattern: "^[\\s\\.\\,!?;:\"'`]+|[\\s\\.\\,!?;:\"'`]+$")
    }()

    /// Strip fillers + restart prefixes + heavy suffixes; collapse whitespace.
    public static func strip(_ text: String) -> String {
        guard !text.isEmpty else { return "" }
        var s = text

        s = replaceAll(in: s, regex: restartVerbPrefix, replacement: "")

        var prev: String? = nil
        while prev != s {
            prev = s
            s = replaceAll(in: s, regex: heavySuffix, replacement: "")
        }

        var lowered = s.lowercased()
        lowered = compoundPass(lowered)

        let kept = lowered.split(separator: " ", omittingEmptySubsequences: true)
            .map(String.init)
            .filter { token in
                let trimmed = token.trimmingCharacters(in: CharacterSet(charactersIn: ".,?!'\"`"))
                return !singleFillers.contains(trimmed)
            }
        s = kept.joined(separator: " ")
        s = compoundPass(s)
        s = replaceAll(in: s, regex: whitespace, replacement: " ")
        s = replaceAll(in: s, regex: punctuationEdge, replacement: "")
        return s.trimmingCharacters(in: .whitespaces)
    }

    private static func compoundPass(_ input: String) -> String {
        var s = input
        for compound in compoundFillers {
            let pattern = "\\b\(NSRegularExpression.escapedPattern(for: compound))\\b"
            if let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) {
                s = replaceAll(in: s, regex: re, replacement: " ")
            }
        }
        return s
    }

    private static func replaceAll(in input: String, regex: NSRegularExpression, replacement: String) -> String {
        let range = NSRange(input.startIndex..., in: input)
        return regex.stringByReplacingMatches(in: input, options: [], range: range, withTemplate: replacement)
    }
}
