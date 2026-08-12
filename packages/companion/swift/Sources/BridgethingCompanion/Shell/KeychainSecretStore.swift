#if canImport(Security)

    import BridgethingCompanionCore
    import Foundation
    import Security
    import os

    private let secretsLog = Logger(subsystem: "com.bridgething.companion", category: "secrets")

    public final class KeychainSecretStore: SecretStore, @unchecked Sendable {
        static let service = "com.bridgething.secrets"

        public init() {}

        public func get(key: String) -> String? {
            guard let data = Self.read(service: Self.service, account: key) else { return nil }
            return String(data: data, encoding: .utf8)
        }

        public func set(key: String, value: String) {
            Self.write(service: Self.service, account: key, data: Data(value.utf8))
        }

        public func remove(key: String) {
            Self.delete(service: Self.service, account: key)
        }

        public func getBlob(key: String) -> Data? {
            Self.read(service: Self.service, account: key)
        }

        static func read(service: String, account: String) -> Data? {
            let query: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecAttrAccount as String: account,
                kSecMatchLimit as String: kSecMatchLimitOne,
                kSecReturnData as String: true,
            ]
            var item: CFTypeRef?
            let status = SecItemCopyMatching(query as CFDictionary, &item)
            if status != errSecSuccess {
                if status != errSecItemNotFound {
                    secretsLog.error("keychain read failed for \(account, privacy: .public): \(status)")
                }
                return nil
            }
            return item as? Data
        }

        static func write(service: String, account: String, data: Data) {
            let query: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecAttrAccount as String: account,
            ]
            let attrs: [String: Any] = [
                kSecValueData as String: data,
                kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            ]
            var status = SecItemUpdate(query as CFDictionary, attrs as CFDictionary)
            if status == errSecItemNotFound {
                var insert = query
                insert.merge(attrs) { _, new in new }
                status = SecItemAdd(insert as CFDictionary, nil)
            }
            if status != errSecSuccess {
                secretsLog.error("keychain write failed for \(account, privacy: .public): \(status)")
            }
        }

        static func delete(service: String, account: String) {
            SecItemDelete([
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecAttrAccount as String: account,
            ] as CFDictionary)
        }
    }

#endif
