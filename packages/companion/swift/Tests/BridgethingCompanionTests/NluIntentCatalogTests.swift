import Foundation
import Testing

@testable import BridgethingCompanion

@Suite("nlu intent catalog")
struct NluIntentCatalogTests {
    @Test("catalog matches the decoding grammar")
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
        let catalog = Set(NluIntentCatalog.surfaceNames)
        #expect(grammarIntents.subtracting(catalog).isEmpty,
                "grammar admits intents the catalog never lists: \(grammarIntents.subtracting(catalog).sorted())")
        #expect(catalog.subtracting(grammarIntents).isEmpty,
                "catalog lists intents the grammar rejects: \(catalog.subtracting(grammarIntents).sorted())")
    }

    @Test("label indices are unique and stable")
    func labelIndices() {
        let names = NluIntentCatalog.surfaceNames
        #expect(Set(names).count == names.count, "duplicate intent names would collide label indices")
        #expect(names == names.sorted(), "label order must stay alphabetical to match the exported head")
        #expect(NluIntentCatalog.name(at: 0) == names.first)
        #expect(NluIntentCatalog.name(at: names.count) == nil)
    }

    @Test("rejection wire values are not model classes")
    func rejectionValuesExcluded() {
        #expect(!NluIntentCatalog.contains(NluIntentCatalog.noIntent))
        #expect(!NluIntentCatalog.contains(NluIntentCatalog.clarify))
    }
}
