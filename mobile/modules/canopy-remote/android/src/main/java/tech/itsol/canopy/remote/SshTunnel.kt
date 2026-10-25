package tech.itsol.canopy.remote

import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.userauth.password.PasswordUtils
import java.io.Closeable
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

// The application owns this client and loopback listener. No shell, known_hosts or SSH agent.
internal class SshTunnel(private val config: Map<String, String>) : Closeable {
  private val closed = AtomicBoolean(false)
  private val ssh = SSHClient()
  private val listener = ServerSocket(0, 1, InetAddress.getByName("127.0.0.1"))
  @Volatile private var socket: Socket? = null
  private var worker: Thread? = null
  val port: Int get() = listener.localPort

  fun start() {
    val pin = config.getValue("sshFingerprint")
    require(pin.matches(Regex("SHA256:[A-Za-z0-9+/]{43}=?"))) { "SHA256 SSH host key pin required" }
    ssh.addHostKeyVerifier(pin)
    ssh.connectTimeout = 10000
    ssh.timeout = 10000
    try {
      ssh.connect(config.getValue("sshAddress"), config.getValue("sshPort").toInt())
      check(!closed.get()) { "tunnel cancelled" }
      if (config.containsKey("sshPrivateKey")) {
        val passphrase = config["sshPassphrase"]?.toCharArray()
        val provider = ssh.loadKeys(config.getValue("sshPrivateKey"), config["sshPublicKey"], passphrase?.let { PasswordUtils.createOneOff(it) })
        ssh.authPublickey(config.getValue("sshUsername"), provider)
      } else {
        ssh.authPassword(config.getValue("sshUsername"), config.getValue("sshPassword").toCharArray())
      }
      check(!closed.get()) { "tunnel cancelled" }
      worker = thread(name = "canopy-ssh-tunnel", isDaemon = true) {
        try {
          while (!closed.get()) {
            // One HTTP/2 connection per host; no unbounded accepted socket/channel pool.
            val local = listener.accept()
            socket = local
            local.tcpNoDelay = true
            val remote = ssh.newDirectConnection(config.getValue("address"), config.getValue("port").toInt())
            try {
              val upload = thread(name = "canopy-ssh-write", isDaemon = true) {
                try { local.getInputStream().copyTo(remote.outputStream, 32768) }
                catch (_: Exception) { /* The gRPC call reports the transport failure. */ }
                finally { runCatching { remote.close() }; runCatching { local.close() } }
              }
              try { remote.inputStream.copyTo(local.getOutputStream(), 32768) }
              finally { runCatching { remote.close() }; runCatching { local.close() }; upload.join(1000) }
            } finally { runCatching { remote.close() }; runCatching { local.close() }; socket = null }
          }
        } catch (_: Exception) { /* Pending gRPC calls fail; foreground reconnect creates a new tunnel. */ }
        finally { close() }
      }
    } catch (error: Exception) { close(); throw error }
  }
  override fun close() {
    if (!closed.compareAndSet(false, true)) return
    runCatching { listener.close() }
    runCatching { socket?.close() }
    runCatching { ssh.close() }
    worker?.interrupt()
  }
}
