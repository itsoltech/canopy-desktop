import ExpoModulesCore
import Foundation
import CryptoKit

public class CanopyRemoteModule: Module {
  private let vault = CanopyVault()
  private let attachments = CanopyAttachments()
  private let transport = CanopyGrpc()
  private var hashes: [String: SHA256] = [:]
  private let hashLock = NSLock()
  private func checked(_ error: String?) throws {
    if let error { throw NSError(domain: "CanopyRemote", code: 1, userInfo: [NSLocalizedDescriptionKey: error]) }
  }
  private func stateURL(_ key: String) throws -> URL {
    guard key.range(of: "^[a-zA-Z0-9_-]{1,128}$", options: .regularExpression) != nil else {
      throw NSError(domain: "CanopyRemote", code: 2)
    }
    var dir = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true).appendingPathComponent("CanopyRemote", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    var values = URLResourceValues(); values.isExcludedFromBackup = true
    try dir.setResourceValues(values)
    return dir.appendingPathComponent(key + ".json")
  }
  public func definition() -> ModuleDefinition {
    Name("CanopyRemote")
    Events("rpc")
    View(TerminalKeyboardView.self) { Events("onTerminalKey") }
    OnCreate { [weak self] in
      self?.transport.event = { [weak self] event in self?.sendEvent("rpc", event as! [String: Any]) }
    }
    AsyncFunction("cleanupAttachments") { (keep: [String]) -> Int in try self.attachments.cleanup(keep) }
    AsyncFunction("beginAttachment") { (id: String) in try self.attachments.begin(id) }
    AsyncFunction("appendAttachment") { (id: String, base64: String) in try self.attachments.append(id, base64: base64) }
    AsyncFunction("finishAttachment") { (id: String, media: String) -> [String: Any] in try self.attachments.finish(id, mediaType: media) }
    AsyncFunction("readAttachment") { (id: String, offset: Int, length: Int) -> String in try self.attachments.read(id, offset: offset, length: length) }
    AsyncFunction("attachmentUri") { (id: String) -> String in try self.attachments.uri(id) }
    AsyncFunction("removeAttachment") { (id: String) in try self.attachments.remove(id) }
    AsyncFunction("pickImage") { (promise: Promise) in
      guard let controller = self.appContext?.utilities?.currentViewController() else { promise.reject("NO_VIEW", "No active view"); return }
      self.attachments.pick(from: controller) { result in
        switch result { case .success(let source): promise.resolve(source); case .failure(let error): promise.reject(error) }
      }
    }.runOnQueue(.main)
    AsyncFunction("pickFile") { (promise: Promise) in
      guard let controller = self.appContext?.utilities?.currentViewController() else { promise.reject("NO_VIEW", "No active view"); return }
      self.attachments.pick(from: controller, imagesOnly: false) { result in
        switch result { case .success(let source): promise.resolve(source); case .failure(let error): promise.reject(error) }
      }
    }.runOnQueue(.main)
    AsyncFunction("createIdentity") { (ref: String) -> [String: String] in
      if let saved = try? self.vault.load(ref), let csr = saved["csr"] { return ["csr": csr] }
      let result = self.transport.createIdentity()
      try self.checked(result["error"])
      try self.vault.save(ref, value: result)
      return ["csr": result["csr"]!]
    }
    AsyncFunction("installIdentity") { (pending: String, ref: String, chain: String) in
      let identity = try self.vault.load(pending)
      try self.vault.save(ref, value: ["privateKey": identity["privateKey"]!, "certificateChain": chain])
    }
    AsyncFunction("saveSshCredential") { (ref: String, value: [String: String]) in try self.vault.save(ref, value: value) }
    AsyncFunction("removeCredential") { (ref: String) in try self.vault.remove(ref) }
    AsyncFunction("connect") { (id: String, config: [String: String]) in
      var resolved = config
      if let ref = config["credentialRef"] { resolved.merge(try self.vault.load(ref)) { _, new in new } }
      if let ref = config["sshCredentialRef"] { resolved.merge(try self.vault.load(ref)) { _, new in new } }
      try self.checked(self.transport.connect(id, config: resolved))
    }
    AsyncFunction("start") { (host: String, id: String, path: String, deadline: Double) in try self.checked(self.transport.start(host, callId: id, path: path, deadline: deadline)) }
    AsyncFunction("write") { (id: String, data: String) in try self.checked(self.transport.write(id, data: data)) }
    AsyncFunction("ack") { (id: String) in try self.checked(self.transport.ack(id)) }
    AsyncFunction("finish") { (id: String) in try self.checked(self.transport.finish(id)) }
    AsyncFunction("cancel") { (id: String) in self.transport.cancel(id) }
    AsyncFunction("disconnect") { (id: String) in self.transport.disconnect(id) }
    AsyncFunction("startHash") { (id: String) in
      self.hashLock.lock(); defer { self.hashLock.unlock() }
      guard self.hashes.count < 32, self.hashes[id] == nil else { throw NSError(domain: "CanopyRemote", code: 8) }
      self.hashes[id] = SHA256()
    }
    AsyncFunction("hashChunk") { (id: String, base64: String) in
      self.hashLock.lock(); defer { self.hashLock.unlock() }
      guard base64.count <= 87384, let data = Data(base64Encoded: base64), var hash = self.hashes[id] else { throw NSError(domain: "CanopyRemote", code: 8) }
      hash.update(data: data); self.hashes[id] = hash
    }
    AsyncFunction("finishHash") { (id: String) -> String in
      self.hashLock.lock(); defer { self.hashLock.unlock() }
      guard let hash = self.hashes.removeValue(forKey: id) else { throw NSError(domain: "CanopyRemote", code: 8) }
      return Data(hash.finalize()).base64EncodedString()
    }
    AsyncFunction("cancelHash") { (id: String) in
      self.hashLock.lock(); defer { self.hashLock.unlock() }
      self.hashes.removeValue(forKey: id)
    }
    AsyncFunction("readState") { (key: String) -> String? in
      let url = try self.stateURL(key)
      guard FileManager.default.fileExists(atPath: url.path) else { return nil }
      return try String(contentsOf: url, encoding: .utf8)
    }
    AsyncFunction("writeState") { (key: String, json: String) in
      guard json.utf8.count <= 64 * 1024 * 1024 else { throw NSError(domain: "CanopyRemote", code: 8) }
      let url = try self.stateURL(key)
      try Data(json.utf8).write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
    OnDestroy { self.transport.shutdown(); self.hashLock.lock(); self.hashes.removeAll(); self.hashLock.unlock() }
  }
}
