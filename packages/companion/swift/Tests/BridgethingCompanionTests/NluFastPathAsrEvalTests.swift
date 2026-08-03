import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("nlu fast path vs real asr")
struct NluFastPathAsrEvalTests {
    static let expressibleSlots: [String: Set<String>] = [
        "PLAY": ["target_type"],
        "PAUSE": [],
        "NEXT": [],
        "PREVIOUS": [],
        "SET_VOLUME": ["level", "direction", "amount", "mute"],
        "SET_SHUFFLE": ["enabled"],
        "SET_REPEAT": ["repeat_mode"],
        "SET_PLAYBACK_SPEED": ["speed"],
        "SEEK_RELATIVE": ["seconds"],
        "PRESET_PLAY": ["preset"],
        "PRESET_SAVE": ["preset"],
        "SHOW_VIEW": ["view"],
    ]

    static let scored: Set<String> = ["PAUSE", "NEXT", "PREVIOUS", "SET_SHUFFLE", "SET_REPEAT"]

    static func expressible(_ gold: Gold) -> Bool {
        guard let allowed = expressibleSlots[gold.intent] else { return false }
        guard gold.slots.keys.allSatisfy({ allowed.contains($0) }) else { return false }
        if gold.intent == "SHOW_VIEW" {
            return gold.slots["view"] == .text("now_playing")
        }
        return true
    }

    enum SlotValue: Decodable, Equatable {
        case text(String)
        case flag(Bool)
        case number(Int)

        init(from decoder: Decoder) throws {
            let c = try decoder.singleValueContainer()
            if let b = try? c.decode(Bool.self) {
                self = .flag(b)
            } else if let n = try? c.decode(Int.self) {
                self = .number(n)
            } else {
                self = .text(try c.decode(String.self))
            }
        }
    }

    struct Gold: Decodable {
        let intent: String
        let slots: [String: SlotValue]
    }

    struct Row: Decodable {
        let id: String
        let utterance: String
        let reference: String
        let gold: Gold
    }

    enum Outcome {
        case correct
        case slotsWrong
        case intentWrong
        case declined
    }

    static func slotsAgree(_ hit: NluFastPath.Hit, _ gold: Gold) -> Bool {
        for (key, value) in gold.slots {
            switch (key, value) {
            case ("repeat_mode", .text(let want)):
                guard hit.slots.repeatMode?.rawValue == want else { return false }
            case ("enabled", .flag(let want)):
                guard hit.slots.enabled == want else { return false }
            case ("mute", .flag(let want)):
                guard hit.slots.mute == want else { return false }
            case ("preset", .text(let want)):
                guard hit.slots.preset == want else { return false }
            case ("level", .number(let want)):
                guard hit.slots.level.map(Int.init) == want else { return false }
            case ("seconds", .number(let want)):
                guard hit.slots.seconds.map(Int.init) == want else { return false }
            case ("speed", .text(let want)):
                guard hit.slots.speed?.rawValue == want else { return false }
            case ("direction", .text(let want)):
                guard hit.slots.direction?.rawValue == want else { return false }
            case ("amount", .text(let want)):
                guard hit.slots.amount?.rawValue == want else { return false }
            case ("view", .text("now_playing")):
                guard hit.slots.view == .nowPlaying else { return false }
            case ("target_type", .text):
                guard hit.intent == "PLAY" else { return false }
            default:
                return false
            }
        }
        return true
    }

    static func score(_ transcript: String, _ gold: Gold) -> Outcome {
        guard let hit = NluFastPath.match(transcript) else { return .declined }
        guard hit.intent == gold.intent else { return .intentWrong }
        return slotsAgree(hit, gold) ? .correct : .slotsWrong
    }

    struct Tally {
        var correct = 0
        var slotsWrong = 0
        var intentWrong = 0
        var declined = 0

        var total: Int { correct + slotsWrong + intentWrong + declined }
        var fired: Int { correct + slotsWrong + intentWrong }

        mutating func add(_ o: Outcome) {
            switch o {
            case .correct: correct += 1
            case .slotsWrong: slotsWrong += 1
            case .intentWrong: intentWrong += 1
            case .declined: declined += 1
            }
        }
    }

    static func pct(_ n: Int, _ d: Int) -> String {
        d == 0 ? "n/a" : String(format: "%5.1f%%", 100.0 * Double(n) / Double(d))
    }

    static func loadRows() throws -> [Row]? {
        guard let path = ProcessInfo.processInfo.environment["BRIDGETHING_ASR_EVAL"] else {
            return nil
        }
        let text = try String(contentsOfFile: path, encoding: .utf8)
        let decoder = JSONDecoder()
        return try text.split(separator: "\n").compactMap { line -> Row? in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty else { return nil }
            return try decoder.decode(Row.self, from: Data(trimmed.utf8))
        }
    }

    @Test("scores the fast path on recognizer hypotheses and clean references")
    func asrRoundTrip() throws {
        guard let rows = try Self.loadRows() else {
            print("BRIDGETHING_ASR_EVAL unset; skipping")
            return
        }

        print("rows: \(rows.count)\n")

        for (label, pick) in [
            ("hypothesis (whisper)", \Row.utterance),
            ("reference (clean)", \Row.reference),
        ] as [(String, KeyPath<Row, String>)] {
            var byIntent: [String: Tally] = [:]
            for row in rows {
                byIntent[row.gold.intent, default: Tally()].add(
                    Self.score(row[keyPath: pick], row.gold))
            }

            print("== \(label) ==")
            print("  intent            n   correct  slotWrong intentWrong  declined")
            for intent in byIntent.keys.sorted() {
                let t = byIntent[intent]!
                let name = intent.padding(toLength: 16, withPad: " ", startingAt: 0)
                print(
                    "  \(name) \(String(format: "%4d", t.total))"
                        + "    \(Self.pct(t.correct, t.total))"
                        + "     \(Self.pct(t.slotsWrong, t.total))"
                        + "      \(Self.pct(t.intentWrong, t.total))"
                        + "     \(Self.pct(t.declined, t.total))")
            }

            var recallHit = 0
            var recallTotal = 0
            var wrongFire = 0
            var mustNotFireTotal = 0
            var partialFire = 0
            for row in rows {
                let outcome = Self.score(row[keyPath: pick], row.gold)
                if Self.scored.contains(row.gold.intent), Self.expressible(row.gold) {
                    recallTotal += 1
                    if outcome == .correct { recallHit += 1 }
                }
                if !Self.expressible(row.gold) {
                    mustNotFireTotal += 1
                    switch outcome {
                    case .declined: break
                    case .slotsWrong, .correct: partialFire += 1
                    case .intentWrong: wrongFire += 1
                    }
                }
            }
            var served = 0
            for row in rows where Self.score(row[keyPath: pick], row.gold) == .correct {
                served += 1
            }
            let needsModel = rows.filter {
                $0.gold.intent != "NO_INTENT" && Self.score($0[keyPath: pick], $0.gold) != .correct
            }.count

            print("")
            print("  END TO END served correctly: \(served)/\(rows.count) \(Self.pct(served, rows.count))")
            print("  real commands needing the model: \(needsModel)/\(rows.count) \(Self.pct(needsModel, rows.count))")
            print("  recall on scored classes: \(recallHit)/\(recallTotal) \(Self.pct(recallHit, recallTotal))")
            print("  wrong fire on must-decline: \(wrongFire)/\(mustNotFireTotal) \(Self.pct(wrongFire, mustNotFireTotal))")
            print("  partial fire (same intent, slot underserved): \(partialFire)/\(mustNotFireTotal)")
            print("")

            if pick == \Row.utterance {
                #expect(wrongFire == 0, "\(label): fast path claimed \(wrongFire) utterances it cannot serve")
                #expect(partialFire <= 2, "\(label): same-intent partial fires grew past the measured count")
            }
            #expect(
                Double(recallHit) / Double(recallTotal) >= 0.65,
                "\(label): recall regressed below the measured floor")
        }
    }

    @Test("lists every fire the fast path should have declined")
    func wrongFireDetail() throws {
        guard let rows = try Self.loadRows() else {
            print("BRIDGETHING_ASR_EVAL unset; skipping")
            return
        }
        for (label, pick) in [
            ("recognizer hypotheses", \Row.utterance),
            ("clean references", \Row.reference),
        ] as [(String, KeyPath<Row, String>)] {
            print("== fires on must-decline, \(label) ==")
            var shown = 0
            for row in rows {
                guard !Self.expressible(row.gold) else { continue }
                let text = row[keyPath: pick]
                guard let hit = NluFastPath.match(text) else { continue }
                shown += 1
                print("  gold=\(row.gold.intent) got=\(hit.intent)  \(text.debugDescription)")
            }
            print("  total: \(shown)")
        }
    }

    @Test("lists misses on the scored reachable classes")
    func missDetail() throws {
        guard let rows = try Self.loadRows() else {
            print("BRIDGETHING_ASR_EVAL unset; skipping")
            return
        }
        print("== misses on scored classes (recognizer hypotheses) ==")
        var counts: [String: Int] = [:]
        for row in rows where Self.scored.contains(row.gold.intent) && Self.expressible(row.gold) {
            let outcome = Self.score(row.utterance, row.gold)
            guard outcome != .correct else { continue }
            counts[row.gold.intent, default: 0] += 1
            let refOutcome = Self.score(row.reference, row.gold)
            let blame = refOutcome == .correct ? "asr" : "rules"
            print("  [\(blame)] gold=\(row.gold.intent) \(row.utterance.debugDescription)")
        }
        print("  by intent: \(counts.sorted { $0.key < $1.key })")
    }
}
