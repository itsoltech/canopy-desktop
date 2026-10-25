import Foundation
import CryptoKit
import UIKit
import UniformTypeIdentifiers

final class CanopyAttachments: NSObject, UIDocumentPickerDelegate {
  private let lock = NSRecursiveLock()
  private var imagesOnly = true
  private var pickerResult: ((Result<[String: Any], Error>) -> Void)?
  private func failure(_ text: String) -> NSError { NSError(domain: "CanopyAttachments", code: 8, userInfo: [NSLocalizedDescriptionKey: text]) }
  private func directory() throws -> URL {
    var dir = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true).appendingPathComponent("CanopyAttachments", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    var values = URLResourceValues(); values.isExcludedFromBackup = true; try dir.setResourceValues(values)
    return dir
  }
  private func file(_ id: String, partial: Bool = false) throws -> URL {
    guard id.range(of: "^[a-zA-Z0-9_-]{1,100}$", options: .regularExpression) != nil else { throw failure("Invalid attachment ID") }
    return try directory().appendingPathComponent(id + (partial ? ".partial" : ".data"))
  }
  func cleanup(_ keep: [String]) throws -> Int {
    lock.lock(); defer { lock.unlock() }
    let retained = Set(keep); var removed = 0
    for url in try FileManager.default.contentsOfDirectory(at: directory(), includingPropertiesForKeys: [.contentModificationDateKey]) {
      let id = url.deletingPathExtension().lastPathComponent
      let date = try url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate ?? Date()
      if !retained.contains(id) && date.timeIntervalSinceNow < -3600 {
        try FileManager.default.removeItem(at: url); removed += 1
      }
    }
    return removed
  }
  func begin(_ id: String) throws {
    lock.lock(); defer { lock.unlock() }
    let url = try file(id, partial: true)
    guard !FileManager.default.fileExists(atPath: url.path), !FileManager.default.fileExists(atPath: try file(id).path) else { throw failure("Attachment already exists") }
    try Data().write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
  }
  func append(_ id: String, base64: String) throws {
    lock.lock(); defer { lock.unlock() }
    guard base64.utf8.count <= 87384, let bytes = Data(base64Encoded: base64), bytes.count <= 65536 else { throw failure("Invalid attachment chunk") }
    let url = try file(id, partial: true)
    let own = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
    let files = try FileManager.default.contentsOfDirectory(at: directory(), includingPropertiesForKeys: [.fileSizeKey])
    let total = try files.reduce(0) { try $0 + ($1.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) }
    guard own + bytes.count <= 64 * 1024 * 1024, total + bytes.count <= 256 * 1024 * 1024 else { throw failure("Local attachment storage is full") }
    let handle = try FileHandle(forWritingTo: url); defer { try? handle.close() }
    try handle.seekToEnd(); try handle.write(contentsOf: bytes); try handle.synchronize()
  }
  func finish(_ id: String, mediaType: String) throws -> [String: Any] {
    lock.lock(); defer { lock.unlock() }
    let source = try file(id, partial: true), target = try file(id)
    let handle = try FileHandle(forReadingFrom: source); defer { try? handle.close() }
    var hash = SHA256(); var size = 0
    while let bytes = try handle.read(upToCount: 65536), !bytes.isEmpty { hash.update(data: bytes); size += bytes.count }
    try FileManager.default.moveItem(at: source, to: target)
    return ["id": id, "size": size, "sha256": Data(hash.finalize()).base64EncodedString(), "mediaType": mediaType]
  }
  func read(_ id: String, offset: Int, length: Int) throws -> String {
    lock.lock(); defer { lock.unlock() }
    guard offset >= 0, length > 0, length <= 65536 else { throw failure("Invalid attachment range") }
    let handle = try FileHandle(forReadingFrom: file(id)); defer { try? handle.close() }
    try handle.seek(toOffset: UInt64(offset)); return (try handle.read(upToCount: length) ?? Data()).base64EncodedString()
  }
  func uri(_ id: String) throws -> String { return try file(id).absoluteString }
  func remove(_ id: String) throws {
    lock.lock(); defer { lock.unlock() }
    for partial in [true, false] { let url = try file(id, partial: partial); if FileManager.default.fileExists(atPath: url.path) { try FileManager.default.removeItem(at: url) } }
  }
  func pick(from controller: UIViewController, imagesOnly: Bool = true, completion: @escaping (Result<[String: Any], Error>) -> Void) {
    guard pickerResult == nil else { completion(.failure(failure("Image picker already open"))); return }
    pickerResult = completion; self.imagesOnly = imagesOnly
    let picker = UIDocumentPickerViewController(forOpeningContentTypes: imagesOnly ? [.image] : [.item], asCopy: true)
    picker.delegate = self; picker.allowsMultipleSelection = false; controller.present(picker, animated: true)
  }
  func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) { pickerResult?(.failure(failure("Image selection cancelled"))); pickerResult = nil }
  func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
    let completion = pickerResult; pickerResult = nil
    guard let url = urls.first else { completion?(.failure(failure("No image selected"))); return }
    let imagesOnly = self.imagesOnly
    DispatchQueue.global(qos: .userInitiated).async {
      let id = UUID().uuidString
      do {
        let scoped = url.startAccessingSecurityScopedResource(); defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let type = try url.resourceValues(forKeys: [.contentTypeKey]).contentType
        guard let type, (!imagesOnly || type.conforms(to: .image)), let media = type.preferredMIMEType else { throw self.failure("Unsupported image type") }
        try self.begin(id)
        let input = try FileHandle(forReadingFrom: url); defer { try? input.close() }
        while let bytes = try input.read(upToCount: 65536), !bytes.isEmpty { try self.append(id, base64: bytes.base64EncodedString()) }
        completion?(.success(try self.finish(id, mediaType: media)))
      } catch { try? self.remove(id); completion?(.failure(error)) }
    }
  }
}
