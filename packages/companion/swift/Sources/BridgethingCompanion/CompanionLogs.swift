import BridgethingCompanionCore
import Foundation

public final class CompanionLogs: @unchecked Sendable {
  public static let shared = CompanionLogs()

  private let lock = NSLock()
  private var installed: LogStore?

  private init() {}

  public static func defaultRoot() -> URL {
    let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
      ?? FileManager.default.temporaryDirectory
    return base.appendingPathComponent("bridgething-logs", isDirectory: true)
  }

  @discardableResult
  public func install(root: URL = CompanionLogs.defaultRoot()) -> LogStore {
    lock.lock()
    defer { lock.unlock() }
    if let installed { return installed }
    try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let store = LogStore.install(root: root.path)
    installed = store
    return store
  }

  public var store: LogStore? {
    lock.lock()
    defer { lock.unlock() }
    return installed
  }
}
