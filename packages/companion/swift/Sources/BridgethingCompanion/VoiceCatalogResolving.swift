public protocol VoiceCatalogResolving: Sendable {
    func decorate(_ prediction: NluPrediction) async throws -> NluPrediction
}

public protocol VoiceCatalogProviding: Sendable {
    func voiceResolver() -> (any VoiceCatalogResolving)?
}
