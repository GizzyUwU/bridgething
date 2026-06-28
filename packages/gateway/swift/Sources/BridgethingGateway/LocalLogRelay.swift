import Foundation

public final class LocalLogRelay: @unchecked Sendable {
  public static let shared = LocalLogRelay()

  private let lock = NSLock()
  private var sink: (@Sendable (String, String, String) -> Void)?

  private init() {}

  public func setSink(_ sink: (@Sendable (String, String, String) -> Void)?) {
    lock.lock()
    defer { lock.unlock() }
    self.sink = sink
  }

  public func push(level: String, target: String, message: String) {
    lock.lock()
    let sink = self.sink
    lock.unlock()
    sink?(level, target, message)
  }
}
