package tech.itsol.canopy.remote

import android.util.AtomicFile
import android.util.Base64
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import io.grpc.*
import io.grpc.okhttp.OkHttpChannelBuilder
import java.io.ByteArrayInputStream
import java.io.InputStream
import java.io.File
import org.bouncycastle.pkcs.jcajce.JcaPKCS10CertificationRequestBuilder
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder
import java.security.KeyPairGenerator
import java.security.spec.ECGenParameterSpec
import javax.security.auth.x500.X500Principal
import java.security.KeyFactory
import java.security.KeyStore
import java.security.MessageDigest
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.PKCS8EncodedKeySpec
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import javax.net.ssl.*

class CanopyRemoteModule : Module() {
  private val vault by lazy { Vault(requireNotNull(appContext.reactContext)) }
  private val attachments by lazy { Attachments(requireNotNull(appContext.reactContext)) }
  private var imagePromise: expo.modules.kotlin.Promise? = null
  private val hashes = ConcurrentHashMap<String, MessageDigest>()
  private val tunnels = ConcurrentHashMap<String, SshTunnel>()
  private val channels = ConcurrentHashMap<String, ManagedChannel>()
  private val buffered = ConcurrentHashMap<String, AtomicLong>()
  private val calls = ConcurrentHashMap<String, Active>()
  private class Active(val host: String, val call: ClientCall<ByteArray, ByteArray>, val bidi: Boolean) {
    var incomingBytes = 0L
    var outstanding = false
    var pending: ByteArray? = null
    var sent = false
    var ended = false
  }
  private val marshaller = object : MethodDescriptor.Marshaller<ByteArray> {
    override fun stream(value: ByteArray): InputStream = ByteArrayInputStream(value)
    override fun parse(stream: InputStream): ByteArray {
      val bytes = stream.readBytes()
      require(bytes.size <= 1024 * 1024) { "resource_exhausted" }
      return bytes
    }
  }
  override fun definition() = ModuleDefinition {
    Name("CanopyRemote")
    Events("rpc")
    View(TerminalKeyboardView::class) { Events("onTerminalKey") }
    AsyncFunction("cleanupAttachments") { keep: List<String> -> attachments.cleanup(keep) }
    AsyncFunction("beginAttachment") { id: String -> attachments.begin(id) }
    AsyncFunction("appendAttachment") { id: String, data: String -> attachments.append(id, data) }
    AsyncFunction("finishAttachment") { id: String, media: String -> attachments.finish(id, media) }
    AsyncFunction("readAttachment") { id: String, offset: Double, length: Int -> attachments.read(id, offset.toLong(), length) }
    AsyncFunction("attachmentUri") { id: String -> attachments.uri(id) }
    AsyncFunction("removeAttachment") { id: String -> attachments.remove(id) }
    AsyncFunction("pickImage") { promise: expo.modules.kotlin.Promise ->
      require(imagePromise == null) { "Image picker already open" }
      val activity = requireNotNull(appContext.currentActivity) { "No active activity" }
      imagePromise = promise
      activity.runOnUiThread {
        try { activity.startActivityForResult(android.content.Intent(android.content.Intent.ACTION_OPEN_DOCUMENT).apply { type = "image/*"; addCategory(android.content.Intent.CATEGORY_OPENABLE) }, 54017) }
        catch (e: Exception) { imagePromise = null; promise.reject("PICKER", "Cannot open image picker", e) }
      }
    }
    AsyncFunction("pickFile") { promise: expo.modules.kotlin.Promise ->
      require(imagePromise == null) { "Image picker already open" }
      val activity = requireNotNull(appContext.currentActivity) { "No active activity" }
      imagePromise = promise
      activity.runOnUiThread {
        try { activity.startActivityForResult(android.content.Intent(android.content.Intent.ACTION_OPEN_DOCUMENT).apply { type = "*/*"; addCategory(android.content.Intent.CATEGORY_OPENABLE) }, 54018) }
        catch (e: Exception) { imagePromise = null; promise.reject("PICKER", "Cannot open image picker", e) }
      }
    }
    OnActivityResult { _, payload ->
      if (payload.requestCode == 54017 || payload.requestCode == 54018) {
        val promise = imagePromise; imagePromise = null
        val uri = payload.data?.data
        if (payload.resultCode != android.app.Activity.RESULT_OK || uri == null) promise?.reject("CANCELLED", "Image selection cancelled", null)
        else Thread {
          try { promise?.resolve(attachments.importSource(uri, payload.requestCode == 54017)) }
          catch (e: Exception) { promise?.reject("IMAGE", "Cannot stage image", e) }
        }.start()
      }
    }
    AsyncFunction("createIdentity") { ref: String ->
      val existing = try { vault.load(ref)["csr"] } catch (_: Exception) { null }
      if (existing != null) return@AsyncFunction mapOf("csr" to existing)
      val pair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
      val signer = JcaContentSignerBuilder("SHA256withECDSA").build(pair.private)
      val csr = JcaPKCS10CertificationRequestBuilder(X500Principal("CN=Canopy Device"), pair.public).build(signer)
      val encodedKey = Base64.encodeToString(pair.private.encoded, Base64.NO_WRAP).chunked(64).joinToString("\n")
      vault.save(ref, mapOf("privateKey" to "-----BEGIN PRIVATE KEY-----\n$encodedKey\n-----END PRIVATE KEY-----\n", "csr" to Base64.encodeToString(csr.encoded, Base64.NO_WRAP)))
      mapOf("csr" to Base64.encodeToString(csr.encoded, Base64.NO_WRAP))
    }
    AsyncFunction("installIdentity") { pending: String, ref: String, chain: String -> vault.save(ref, mapOf("privateKey" to vault.load(pending).getValue("privateKey"), "certificateChain" to chain)) }
    AsyncFunction("saveSshCredential") { ref: String, value: Map<String, String> -> vault.save(ref, value) }
    AsyncFunction("removeCredential") { ref: String -> vault.remove(ref) }
    AsyncFunction("connect") { id: String, input: Map<String, String> ->
      val config = input.toMutableMap()
      input["credentialRef"]?.let { config.putAll(vault.load(it)) }
      input["sshCredentialRef"]?.let { config.putAll(vault.load(it)) }
      require(!channels.containsKey(id)) { "connection already exists" }
      val cert = certificate(config.getValue("hostCertificate"))
      val pin = MessageDigest.getInstance("SHA-256").digest(cert.encoded).joinToString("") { "%02x".format(it) }
      require(pin == config.getValue("fingerprint").lowercase()) { "host fingerprint mismatch" }
      cert.checkValidity()
      // Trust only the exact paired leaf, never a public CA or a sibling signed by it.
      val trust = object : X509TrustManager {
        override fun getAcceptedIssuers() = arrayOf(cert)
        override fun checkClientTrusted(chain: Array<X509Certificate>, authType: String) = throw java.security.cert.CertificateException("client verification unavailable")
        override fun checkServerTrusted(chain: Array<X509Certificate>, authType: String) {
          require(chain.isNotEmpty() && MessageDigest.isEqual(chain[0].encoded, cert.encoded)) { "host certificate changed" }
          chain[0].checkValidity()
        }
      }
      val managers = if (config["privateKey"].isNullOrEmpty()) null else {
        val keyBytes = pem(config.getValue("privateKey"), "PRIVATE KEY")
        val key = try { KeyFactory.getInstance("EC").generatePrivate(PKCS8EncodedKeySpec(keyBytes)) }
          catch (_: java.security.GeneralSecurityException) { KeyFactory.getInstance("RSA").generatePrivate(PKCS8EncodedKeySpec(keyBytes)) }
        val store = KeyStore.getInstance(KeyStore.getDefaultType()).apply {
          load(null)
          val chain = CertificateFactory.getInstance("X.509").generateCertificates(config.getValue("certificateChain").byteInputStream()).toTypedArray()
          setKeyEntry("device", key, CharArray(0), chain)
        }
        KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm()).apply { init(store, CharArray(0)) }.keyManagers
      }
      val ssl = SSLContext.getInstance("TLS").apply { init(managers, arrayOf<TrustManager>(trust), null) }
      val tunnel = if (config.containsKey("sshAddress")) SshTunnel(config).also {
        if (tunnels.putIfAbsent(id, it) != null) { it.close(); error("tunnel already exists") }
        try { it.start() } catch (error: Exception) { tunnels.remove(id, it); throw error }
      } else null
      val channel = OkHttpChannelBuilder.forAddress(if (tunnel != null) "127.0.0.1" else config.getValue("address"), tunnel?.port ?: config.getValue("port").toInt())
        .sslSocketFactory(ssl.socketFactory)
        .overrideAuthority(config.getValue("serverName"))
        .maxInboundMessageSize(1024 * 1024).disableRetry()
        .keepAliveTime(30, TimeUnit.SECONDS).keepAliveTimeout(10, TimeUnit.SECONDS)
        .build()
      if (channels.putIfAbsent(id, channel) != null) { channel.shutdownNow(); error("connection already exists") }
    }
    AsyncFunction("start") { host: String, id: String, path: String, deadlineMs: Double ->
      require(deadlineMs > 0 && deadlineMs <= 3600000) { "invalid deadline" }
      require(calls.values.count { it.host == host } < 32) { "resource_exhausted" }
      val kind = methods[path.removePrefix("/")] ?: error("unknown schema method")
      val descriptor = MethodDescriptor.newBuilder(marshaller, marshaller)
        .setFullMethodName(path.removePrefix("/")).setType(MethodDescriptor.MethodType.valueOf(kind)).build()
      val channel = channels[host] ?: error("not connected")
      val call = channel.newCall(descriptor, CallOptions.DEFAULT.withDeadlineAfter(deadlineMs.toLong(), TimeUnit.MILLISECONDS))
      val active = Active(host, call, kind == "BIDI_STREAMING")
      require(calls.putIfAbsent(id, active) == null) { "duplicate call" }
      call.start(object : ClientCall.Listener<ByteArray>() {
        override fun onMessage(message: ByteArray) {
          synchronized(active) {
            active.incomingBytes = message.size.toLong()
            if (buffered.computeIfAbsent(host) { AtomicLong() }.addAndGet(active.incomingBytes) > 4 * 1024 * 1024) {
              active.call.cancel("resync_required: inbound byte limit", null)
              return
            }
            active.outstanding = true
          }
          sendEvent("rpc", mapOf("id" to id, "kind" to "message", "data" to Base64.encodeToString(message, Base64.NO_WRAP)))
        }
        override fun onReady() { flush(id, active) }
        override fun onClose(status: Status, trailers: Metadata) {
          calls.remove(id, active)
          synchronized(active) { buffered[host]?.addAndGet(-active.incomingBytes); active.incomingBytes = 0 }
          sendEvent("rpc", mapOf("id" to id, "kind" to "close", "code" to status.code.value(), "message" to status.code.name))
        }
      }, Metadata())
      call.request(1)
    }
    AsyncFunction("write") { id: String, base64: String ->
      val active = calls[id] ?: error("call closed")
      require(base64.length <= 1398104) { "resource_exhausted" }
      val data = Base64.decode(base64, Base64.NO_WRAP)
      synchronized(active) {
        require(!active.ended && active.pending == null && (active.bidi || !active.sent)) { "write unavailable" }
        active.pending = data
      }
      flush(id, active)
    }
    AsyncFunction("ack") { id: String ->
      calls[id]?.let { active -> synchronized(active) {
        if (active.outstanding) { buffered[active.host]?.addAndGet(-active.incomingBytes); active.incomingBytes = 0; active.outstanding = false; active.call.request(1) }
      } }
    }
    AsyncFunction("finish") { id: String ->
      calls[id]?.let { active -> synchronized(active) {
        require(active.pending == null && !active.ended) { "write pending or call ended" }
        active.ended = true; active.call.halfClose()
      } }
    }
    AsyncFunction("cancel") { id: String -> calls.remove(id)?.call?.cancel("client cancelled", null); Unit }
    AsyncFunction("disconnect") { host: String -> disconnect(host) }
    AsyncFunction("startHash") { id: String ->
      require(hashes.size < 32) { "hash limit" }
      require(hashes.putIfAbsent(id, MessageDigest.getInstance("SHA-256")) == null) { "duplicate hash" }
    }
    AsyncFunction("hashChunk") { id: String, data: String ->
      require(data.length <= 87384) { "chunk limit" }
      val hash = hashes[id] ?: error("hash missing")
      synchronized(hash) { hash.update(Base64.decode(data, Base64.NO_WRAP)) }
    }
    AsyncFunction("finishHash") { id: String ->
      val hash = hashes.remove(id) ?: error("hash missing")
      synchronized(hash) { Base64.encodeToString(hash.digest(), Base64.NO_WRAP) }
    }
    AsyncFunction("cancelHash") { id: String -> hashes.remove(id); Unit }
    AsyncFunction("readState") { key: String ->
      val f = stateFile(key)
      if (f.baseFile.exists()) f.openRead().use { it.readBytes().toString(Charsets.UTF_8) } else null
    }
    AsyncFunction("writeState") { key: String, json: String ->
      val bytes = json.toByteArray(Charsets.UTF_8)
      require(bytes.size <= 64 * 1024 * 1024) { "resource_exhausted" }
      val file = stateFile(key)
      val stream = file.startWrite()
      try { stream.write(bytes); file.finishWrite(stream) } catch (e: Throwable) { file.failWrite(stream); throw e }
    }
    OnDestroy { hashes.clear(); (channels.keys + tunnels.keys).toSet().forEach { disconnect(it) } }
  }
  private fun flush(id: String, active: Active) = synchronized(active) {
    val bytes = active.pending
    if (bytes != null && active.call.isReady) {
      active.pending = null; active.sent = true
      active.call.sendMessage(bytes)
      sendEvent("rpc", mapOf("id" to id, "kind" to "written"))
    }
  }
  private fun disconnect(host: String) {
    calls.entries.filter { it.value.host == host }.forEach { (id, active) ->
      calls.remove(id, active); active.call.cancel("connection closed", null)
    }
    channels.remove(host)?.shutdownNow()
    tunnels.remove(host)?.close()
  }
  private fun stateFile(key: String): AtomicFile {
    require(key.matches(Regex("[a-zA-Z0-9_-]{1,128}"))) { "invalid state key" }
    val context = appContext.reactContext ?: error("context unavailable")
    val directory = File(context.noBackupFilesDir, "canopy-remote").apply { mkdirs() }
    return AtomicFile(File(directory, "$key.json"))
  }
  private fun certificate(pem: String) = CertificateFactory.getInstance("X.509").generateCertificate(pem.byteInputStream()) as X509Certificate
  private fun pem(value: String, type: String): ByteArray = Base64.decode(value.replace("-----BEGIN $type-----", "").replace("-----END $type-----", "").replace(Regex("\\s"), ""), Base64.DEFAULT)
}
