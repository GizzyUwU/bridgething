import BridgethingSchema
import Foundation
#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

/// Minimal OpenRouter chat-completions client for the voice-NLU LLM
/// stage. Issues a single grammar-constrained completion against a model
/// (default: Gemma-4-26B-A4B-IT) and returns the parsed JSON. Pure NLU:
/// no streaming, no tool-use, no retries.
///
/// `responseFormat` carries a strict-grammar schema that keeps the
/// model's output to the closed intent enum.
public actor NluOpenRouterClient {
    public enum ClientError: Error, CustomStringConvertible {
        case missingApiKey
        case httpError(statusCode: Int, body: String)
        case invalidResponse(reason: String)
        case parseFailure(text: String, underlying: Error?)

        public var description: String {
            switch self {
            case .missingApiKey: return "OPENROUTER_API_KEY not set"
            case let .httpError(code, body): return "openrouter http \(code): \(body)"
            case let .invalidResponse(reason): return "openrouter invalid response: \(reason)"
            case let .parseFailure(text, _): return "openrouter parse failure on body: \(text.prefix(200))"
            }
        }
    }

    public struct Completion: Sendable {
        public let text: String
        public let usage: [String: Any]?

        public init(text: String, usage: [String: Any]? = nil) {
            self.text = text
            self.usage = usage
        }
    }

    private let urlSession: URLSession
    private let apiKey: String?
    private let baseURL: URL
    private let referer: String
    private let title: String

    public init(
        urlSession: URLSession = .shared,
        apiKey: String? = ProcessInfo.processInfo.environment["OPENROUTER_API_KEY"],
        baseURL: URL = URL(string: "https://openrouter.ai/api/v1")!,
        referer: String = "https://github.com/JoeyEamigh/bridgething",
        title: String = "bridgething-voice"
    ) {
        self.urlSession = urlSession
        self.apiKey = apiKey
        self.baseURL = baseURL
        self.referer = referer
        self.title = title
    }

    public func chat(
        model: String,
        systemPrompt: String,
        utterance: String,
        responseFormat: [String: Any]? = nil,
        temperature: Double = 0.0,
        maxTokens: Int = 256
    ) async throws -> Completion {
        guard let apiKey, !apiKey.isEmpty else { throw ClientError.missingApiKey }

        var body: [String: Any] = [
            "model": model,
            "messages": [
                ["role": "system", "content": systemPrompt],
                ["role": "user", "content": utterance],
            ],
            "temperature": temperature,
            "max_tokens": maxTokens,
            "reasoning": ["enabled": false],
        ]
        if let responseFormat {
            body["response_format"] = responseFormat
        }

        var req = URLRequest(url: baseURL.appendingPathComponent("chat/completions"))
        req.httpMethod = "POST"
        req.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        req.setValue(referer, forHTTPHeaderField: "HTTP-Referer")
        req.setValue(title, forHTTPHeaderField: "X-Title")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try JSONSerialization.data(withJSONObject: body, options: [])

        let (data, response) = try await urlSession.data(for: req)
        guard let http = response as? HTTPURLResponse else {
            throw ClientError.invalidResponse(reason: "no http status")
        }
        guard (200..<300).contains(http.statusCode) else {
            let bodyText = String(data: data, encoding: .utf8) ?? "<binary>"
            throw ClientError.httpError(statusCode: http.statusCode, body: bodyText)
        }
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw ClientError.parseFailure(text: String(data: data, encoding: .utf8) ?? "", underlying: nil)
        }
        guard let choices = json["choices"] as? [[String: Any]],
              let first = choices.first,
              let message = first["message"] as? [String: Any],
              let content = message["content"] as? String
        else {
            throw ClientError.invalidResponse(reason: "missing choices[0].message.content")
        }
        let usage = json["usage"] as? [String: Any]
        return Completion(text: content, usage: usage)
    }
}
