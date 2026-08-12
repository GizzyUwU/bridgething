import Foundation

#if canImport(Glibc)
    import Glibc
#endif
#if canImport(Darwin)
    import Darwin
#endif

#if canImport(Glibc)
    private let sockStream = Int32(SOCK_STREAM.rawValue)
    private let shutReadWrite = Int32(SHUT_RDWR)
#else
    private let sockStream = SOCK_STREAM
    private let shutReadWrite = SHUT_RDWR
#endif

final class MiniHttpServer: @unchecked Sendable {
    typealias Response = (status: Int, headers: [(String, String)], body: Data)
    typealias Handler = @Sendable (_ method: String, _ path: String, _ body: Data) -> Response

    let port: UInt16
    private let listenFd: Int32
    private let handler: Handler

    init?(handler: @escaping Handler) {
        self.handler = handler
        let fd = socket(AF_INET, sockStream, 0)
        guard fd >= 0 else { return nil }
        var yes: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))

        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = UInt16(0).bigEndian
        addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 16) == 0 else {
            close(fd)
            return nil
        }
        var bound2 = sockaddr_in()
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &bound2) { ptr -> Int32 in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(fd, $0, &len)
            }
        }
        listenFd = fd
        port = UInt16(bigEndian: bound2.sin_port)

        let serve = Thread { [handler] in
            while true {
                let client = accept(fd, nil, nil)
                if client < 0 { break }
                MiniHttpServer.serve(client: client, handler: handler)
            }
        }
        serve.name = "mini-http-server"
        serve.start()
    }

    func stop() {
        shutdown(listenFd, shutReadWrite)
        close(listenFd)
    }

    static func unusedPort() -> UInt16 {
        let fd = socket(AF_INET, sockStream, 0)
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = UInt16(0).bigEndian
        addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))
        _ = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        var out = sockaddr_in()
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &out) { ptr -> Int32 in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(fd, $0, &len)
            }
        }
        close(fd)
        return UInt16(bigEndian: out.sin_port)
    }

    private static func serve(client: Int32, handler: Handler) {
        defer { close(client) }
        var buffer = Data()
        var scratch = [UInt8](repeating: 0, count: 16 * 1024)
        var headerEnd: Int? = nil
        while headerEnd == nil {
            let n = recv(client, &scratch, scratch.count, 0)
            if n <= 0 { return }
            buffer.append(contentsOf: scratch[0 ..< n])
            headerEnd = buffer.range(of: Data("\r\n\r\n".utf8))?.upperBound
            if buffer.count > 1 << 20 { return }
        }
        guard let headerEnd else { return }
        guard let head = String(data: buffer.prefix(headerEnd), encoding: .utf8) else { return }
        let lines = head.components(separatedBy: "\r\n")
        let request = lines.first?.components(separatedBy: " ") ?? []
        guard request.count >= 2 else { return }
        let method = request[0]
        let path = request[1]
        let contentLength = lines
            .first { $0.lowercased().hasPrefix("content-length:") }
            .flatMap { Int($0.dropFirst("content-length:".count).trimmingCharacters(in: .whitespaces)) } ?? 0
        var body = Data(buffer.suffix(from: headerEnd))
        while body.count < contentLength {
            let n = recv(client, &scratch, scratch.count, 0)
            if n <= 0 { break }
            body.append(contentsOf: scratch[0 ..< n])
        }

        let response = handler(method, path, body)
        var out = "HTTP/1.1 \(response.status) X\r\n"
        out += "Content-Length: \(response.body.count)\r\n"
        out += "Connection: close\r\n"
        for (name, value) in response.headers {
            out += "\(name): \(value)\r\n"
        }
        out += "\r\n"
        var payload = Data(out.utf8)
        payload.append(response.body)
        payload.withUnsafeBytes { raw in
            guard var base = raw.bindMemory(to: UInt8.self).baseAddress else { return }
            var remaining = payload.count
            while remaining > 0 {
                let n = send(client, base, remaining, 0)
                if n <= 0 { break }
                base += n
                remaining -= n
            }
        }
    }
}
