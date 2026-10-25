package tech.itsol.canopy.remote

import android.content.Context
import android.net.Uri
import android.util.Base64
import java.io.File
import java.io.RandomAccessFile
import java.security.MessageDigest
import java.util.UUID

internal class Attachments(private val context: Context) {
  private val dir get() = File(context.noBackupFilesDir, "canopy-attachments").apply { mkdirs() }
  private fun file(id: String, partial: Boolean = false): File {
    require(id.matches(Regex("[a-zA-Z0-9_-]{1,100}"))) { "Invalid attachment ID" }
    return File(dir, id + if (partial) ".partial" else ".data")
  }
  @Synchronized fun cleanup(keep: List<String>): Int {
    val retained = keep.toSet(); var removed = 0
    for (file in dir.listFiles() ?: emptyArray()) if (file.nameWithoutExtension !in retained && file.lastModified() < System.currentTimeMillis() - 3600000 && file.delete()) removed++
    return removed
  }
  @Synchronized fun begin(id: String) { require(!file(id).exists() && file(id, true).createNewFile()) { "Attachment already exists" } }
  @Synchronized fun append(id: String, base64: String) {
    require(base64.length <= 87384)
    val bytes = Base64.decode(base64, Base64.NO_WRAP); val target = file(id, true)
    require(bytes.size <= 65536 && target.exists())
    require(target.length() + bytes.size <= 64L * 1024 * 1024 && (dir.listFiles()?.sumOf { it.length() } ?: 0L) + bytes.size <= 256L * 1024 * 1024) { "Local attachment storage is full" }
    java.io.FileOutputStream(target, true).use { it.write(bytes); it.fd.sync() }
  }
  @Synchronized fun finish(id: String, media: String): Map<String, Any> {
    val source = file(id, true); val hash = MessageDigest.getInstance("SHA-256")
    source.inputStream().use { input -> val bytes = ByteArray(65536); while (true) { val n = input.read(bytes); if (n < 0) break; hash.update(bytes, 0, n) } }
    val size = source.length(); require(!file(id).exists() && source.renameTo(file(id))) { "Cannot publish attachment" }
    return mapOf("id" to id, "size" to size, "sha256" to Base64.encodeToString(hash.digest(), Base64.NO_WRAP), "mediaType" to media)
  }
  @Synchronized fun read(id: String, offset: Long, length: Int): String {
    require(offset >= 0 && length in 1..65536)
    return RandomAccessFile(file(id), "r").use { input -> input.seek(offset); val bytes = ByteArray(length); val n = input.read(bytes); Base64.encodeToString(if (n < 0) ByteArray(0) else bytes.copyOf(n), Base64.NO_WRAP) }
  }
  fun uri(id: String): String = android.net.Uri.fromFile(file(id)).toString()
  @Synchronized fun remove(id: String) { file(id).delete(); file(id, true).delete() }
  fun importSource(uri: Uri, imagesOnly: Boolean): Map<String, Any> {
    val media = context.contentResolver.getType(uri) ?: "application/octet-stream"
    require(!imagesOnly || media.startsWith("image/")) { "Unsupported image type" }
    val id = UUID.randomUUID().toString()
    try {
      begin(id)
      context.contentResolver.openInputStream(uri)?.use { input -> val bytes = ByteArray(65536); while (true) { val n = input.read(bytes); if (n < 0) break; append(id, Base64.encodeToString(bytes, 0, n, Base64.NO_WRAP)) } } ?: error("Image unavailable")
      return finish(id, media)
    } catch (e: Exception) { remove(id); throw e }
  }
}
