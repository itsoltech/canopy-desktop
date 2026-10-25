package tech.itsol.canopy.remote

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONObject

internal class Vault(private val context: Context) {
  private val alias = "canopy.remote.vault.v1"
  private fun file(ref: String): AtomicFile {
    require(ref.matches(Regex("[a-zA-Z0-9_-]{1,100}")))
    return AtomicFile(File(File(context.noBackupFilesDir, "canopy-vault").apply { mkdirs() }, ref))
  }
  private fun key(): SecretKey {
    require(!(context.getSystemService(Context.KEYGUARD_SERVICE) as android.app.KeyguardManager).isDeviceLocked) { "Unlock this device to access credentials" }
    val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    (store.getKey(alias, null) as? SecretKey)?.let { return it }
    val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT).setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
    if (android.os.Build.VERSION.SDK_INT >= 28) spec.setUnlockedDeviceRequired(true)
    return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply { init(spec.build()) }.generateKey()
  }
  @Synchronized fun load(ref: String): Map<String, String> {
    val bytes = file(ref).readFully(); require(bytes.size > 12)
    val cipher = Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(0, 12)))
    cipher.updateAAD(ref.toByteArray())
    val json = JSONObject(String(cipher.doFinal(bytes.copyOfRange(12, bytes.size)), Charsets.UTF_8))
    return json.keys().asSequence().associateWith { json.getString(it) }
  }
  @Synchronized fun save(ref: String, value: Map<String, String>) {
    val target = file(ref)
    if (target.baseFile.exists()) { require(load(ref) == value) { "Credential identity conflict" }; return }
    val cipher = Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.ENCRYPT_MODE, key()); cipher.updateAAD(ref.toByteArray())
    val bytes = cipher.iv + cipher.doFinal(JSONObject(value).toString().toByteArray())
    val stream = target.startWrite()
    try { stream.write(bytes); target.finishWrite(stream) } catch (e: Exception) { target.failWrite(stream); throw e }
  }
  @Synchronized fun remove(ref: String) { file(ref).delete() }
}
