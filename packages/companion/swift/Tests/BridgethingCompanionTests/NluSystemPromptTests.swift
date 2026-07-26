import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("nlu system prompt")
struct NluSystemPromptTests {
    @Test("prompt intent list matches the decoding grammar")
    func matchesGrammar() throws {
        let path = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Documents/carthing/nlu/configs/grammar.strict.json")
        guard let data = try? Data(contentsOf: path) else { return }

        let root = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let branches = root?["oneOf"] as? [[String: Any]] ?? []
        let grammarIntents = Set(branches.compactMap { branch -> String? in
            let properties = branch["properties"] as? [String: Any]
            let intent = properties?["intent"] as? [String: Any]
            return intent?["const"] as? String
        })

        #expect(!grammarIntents.isEmpty, "could not parse intents out of the grammar")
        let prompt = Set(NluSystemPrompt.surfaceNames)
        #expect(grammarIntents.subtracting(prompt).isEmpty,
                "grammar admits intents the prompt never lists: \(grammarIntents.subtracting(prompt).sorted())")
        #expect(prompt.subtracting(grammarIntents).isEmpty,
                "prompt lists intents the grammar rejects: \(prompt.subtracting(grammarIntents).sorted())")
    }

    @Test("active webapps append an extensions block")
    func webappBlock() {
        let bare = NluSystemPrompt.build()
        #expect(!bare.contains("Currently active extensions"))

        let withApp = NluSystemPrompt.build(activeWebapps: [.init(id: "timer", voiceGrammar: "set a timer")])
        #expect(withApp.contains("Currently active extensions"))
        #expect(withApp.contains("- timer: set a timer"))
    }
}
