package io.github.vijaykumawat256.firstlight

// Firstlight's secrets on Android (the database key, sign-in tokens, app passwords, the sync code): the Android
// equivalent of the Mac's Keychain. Each value is encrypted with AES-256-GCM under a key that lives in the Android
// Keystore (hardware-backed where the phone has it) and never leaves it; only the encrypted text is kept, in this app's
// private storage. The entry's name is bound in as associated data, so one entry can't be passed off as another.
// Called from Rust (`src-tauri/src/android_secrets.rs`); never from the page.

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class SecretArgs {
    lateinit var key: String
    var value: String? = null
}

@TauriPlugin
class SecretsPlugin(private val activity: Activity) : Plugin(activity) {
    private val prefs by lazy { activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE) }

    private fun masterKey(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }

    @Command
    fun get(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SecretArgs::class.java)
            val out = JSObject()
            val stored = prefs.getString(args.key, null)
            if (stored != null) {
                val raw = Base64.decode(stored, Base64.NO_WRAP)
                val cipher = Cipher.getInstance(TRANSFORM)
                cipher.init(Cipher.DECRYPT_MODE, masterKey(), GCMParameterSpec(TAG_BITS, raw, 0, IV_BYTES))
                cipher.updateAAD(args.key.toByteArray(Charsets.UTF_8))
                out.put("value", String(cipher.doFinal(raw, IV_BYTES, raw.size - IV_BYTES), Charsets.UTF_8))
            }
            invoke.resolve(out)
        } catch (e: Exception) {
            // The class of failure only: never the value.
            invoke.reject("The secure store could not be read (${e.javaClass.simpleName}).")
        }
    }

    @Command
    fun set(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SecretArgs::class.java)
            val cipher = Cipher.getInstance(TRANSFORM)
            cipher.init(Cipher.ENCRYPT_MODE, masterKey())
            cipher.updateAAD(args.key.toByteArray(Charsets.UTF_8))
            val sealed = cipher.iv + cipher.doFinal((args.value ?: "").toByteArray(Charsets.UTF_8))
            if (!prefs.edit().putString(args.key, Base64.encodeToString(sealed, Base64.NO_WRAP)).commit()) {
                throw IllegalStateException("not saved")
            }
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("The secure store could not be written (${e.javaClass.simpleName}).")
        }
    }

    @Command
    fun delete(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SecretArgs::class.java)
            prefs.edit().remove(args.key).commit()
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("The secure store could not be changed (${e.javaClass.simpleName}).")
        }
    }

    companion object {
        private const val KEYSTORE = "AndroidKeyStore"
        private const val ALIAS = "firstlight-secrets"
        private const val PREFS = "firstlight-secrets"
        private const val TRANSFORM = "AES/GCM/NoPadding"
        private const val TAG_BITS = 128
        private const val IV_BYTES = 12
    }
}
