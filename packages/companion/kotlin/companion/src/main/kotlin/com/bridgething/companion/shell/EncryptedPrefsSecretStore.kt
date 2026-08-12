package com.bridgething.companion.shell

import android.content.Context
import android.content.SharedPreferences
import android.util.Base64
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import uniffi.bridgething_companion.SecretStore

public class EncryptedPrefsSecretStore(
    context: Context,
    fileName: String = "com.bridgething.secrets",
) : SecretStore {
    private val appContext = context.applicationContext

    private val prefs: SharedPreferences? by lazy {
        try {
            val masterKey = MasterKey.Builder(appContext)
                .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                .build()
            @Suppress("DEPRECATION")
            EncryptedSharedPreferences.create(
                appContext,
                fileName,
                masterKey,
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )
        } catch (e: Exception) {
            Log.e(TAG, "secret store unavailable", e)
            null
        }
    }

    override fun get(key: String): String? = prefs?.getString(key, null)

    override fun set(key: String, value: String) {
        prefs?.edit()?.putString(key, value)?.apply()
    }

    override fun remove(key: String) {
        prefs?.edit()?.remove(key)?.apply()
    }

    override fun getBlob(key: String): ByteArray? =
        prefs?.getString(key, null)?.let { runCatching { Base64.decode(it, Base64.NO_WRAP) }.getOrNull() }

    private companion object {
        const val TAG = "bridgething.secrets"
    }
}
