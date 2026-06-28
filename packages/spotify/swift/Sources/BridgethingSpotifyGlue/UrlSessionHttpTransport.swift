import Foundation
import Spotify

#if canImport(Darwin)
    final class UrlSessionHttpTransport: HttpTransport, @unchecked Sendable {
        private let session: URLSession

        init() {
            let cfg = URLSessionConfiguration.default
            cfg.waitsForConnectivity = true
            cfg.networkServiceType = .responsiveData
            cfg.shouldUseExtendedBackgroundIdleMode = true
            cfg.timeoutIntervalForRequest = 30
            cfg.timeoutIntervalForResource = 60
            session = URLSession(configuration: cfg)
        }

        func execute(request: HttpRequest, sink: HttpSink) {
            guard let url = URL(string: request.url) else {
                sink.fail(reason: "invalid url: \(request.url)")
                return
            }
            var req = URLRequest(url: url)
            req.httpMethod = Self.method(request.method)
            for header in request.headers {
                req.setValue(header.value, forHTTPHeaderField: header.name)
            }
            if !request.body.isEmpty {
                req.httpBody = request.body
            }
            if request.timeoutMs > 0 {
                req.timeoutInterval = TimeInterval(request.timeoutMs) / 1000.0
            }
            let task = session.dataTask(with: req) { data, response, error in
                if let error {
                    sink.fail(reason: error.localizedDescription)
                    return
                }
                guard let http = response as? HTTPURLResponse else {
                    sink.fail(reason: "non-http response")
                    return
                }
                let headers = http.allHeaderFields.compactMap { key, value -> HttpHeader? in
                    guard let name = key as? String else { return nil }
                    return HttpHeader(name: name, value: String(describing: value))
                }
                sink.complete(response: HttpResponse(
                    status: UInt16(clamping: http.statusCode),
                    headers: headers,
                    body: data ?? Data()
                ))
            }
            task.resume()
        }

        private static func method(_ method: HttpMethod) -> String {
            switch method {
            case .get: "GET"
            case .post: "POST"
            case .put: "PUT"
            }
        }
    }
#endif
