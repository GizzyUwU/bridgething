package com.bridgething.spotify

import java.io.File
import java.security.MessageDigest

internal class ImageDiskCache(private val dir: File, private val maxBytes: Long) {
    private val index = LinkedHashMap<String, Long>(16, 0.75f, true)
    private var totalBytes = 0L

    init {
        dir.mkdirs()
        dir.listFiles()
            ?.sortedBy { it.lastModified() }
            ?.forEach { f ->
                index[f.name] = f.length()
                totalBytes += f.length()
            }
        synchronized(this) { trim() }
    }

    fun get(key: String): ByteArray? {
        val name = nameFor(key)
        synchronized(this) { if (index[name] == null) return null }
        val file = File(dir, name)
        val bytes = runCatching { file.readBytes() }.getOrNull() ?: return null
        file.setLastModified(System.currentTimeMillis())
        return bytes
    }

    fun put(key: String, bytes: ByteArray) {
        if (bytes.size > maxBytes) return
        val name = nameFor(key)
        val file = File(dir, name)
        if (runCatching { file.writeBytes(bytes) }.isFailure) return
        synchronized(this) {
            index.put(name, bytes.size.toLong())?.let { totalBytes -= it }
            totalBytes += bytes.size.toLong()
            trim()
        }
    }

    private fun trim() {
        val it = index.entries.iterator()
        while (totalBytes > maxBytes && it.hasNext()) {
            val entry = it.next()
            File(dir, entry.key).delete()
            totalBytes -= entry.value
            it.remove()
        }
    }

    private fun nameFor(key: String): String =
        MessageDigest.getInstance("SHA-256")
            .digest(key.toByteArray())
            .joinToString("") { "%02x".format(it.toInt() and 0xff) }
}
