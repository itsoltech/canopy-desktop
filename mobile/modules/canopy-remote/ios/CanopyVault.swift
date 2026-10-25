import Foundation
import Security

final class CanopyVault {
  private let lock = NSRecursiveLock()
  private func query(_ ref: String) throws -> [String: Any] {
    guard ref.range(of: "^[a-zA-Z0-9_-]{1,100}$", options: .regularExpression) != nil else { throw NSError(domain: "CanopyVault", code: 1) }
    return [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "tech.itsol.canopy.remote", kSecAttrAccount as String: ref]
  }
  func load(_ ref: String) throws -> [String: String] {
    lock.lock(); defer { lock.unlock() }
    var request = try query(ref); request[kSecReturnData as String] = true; request[kSecMatchLimit as String] = kSecMatchLimitOne
    var result: CFTypeRef?
    let status = SecItemCopyMatching(request as CFDictionary, &result)
    guard status == errSecSuccess, let data = result as? Data, let value = try JSONSerialization.jsonObject(with: data) as? [String: String] else { throw NSError(domain: "CanopyVault", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "Credential unavailable; unlock or pair this device again"]) }
    return value
  }
  func save(_ ref: String, value: [String: String]) throws {
    lock.lock(); defer { lock.unlock() }
    var request = try query(ref)
    request[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
    request[kSecValueData as String] = try JSONSerialization.data(withJSONObject: value)
    let status = SecItemAdd(request as CFDictionary, nil)
    if status == errSecDuplicateItem {
      guard try load(ref) == value else { throw NSError(domain: "CanopyVault", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "Credential identity conflict"]) }
    } else if status != errSecSuccess { throw NSError(domain: "CanopyVault", code: Int(status)) }
  }
  func remove(_ ref: String) throws {
    lock.lock(); defer { lock.unlock() }
    let status = SecItemDelete(try query(ref) as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else { throw NSError(domain: "CanopyVault", code: Int(status)) }
  }
}
