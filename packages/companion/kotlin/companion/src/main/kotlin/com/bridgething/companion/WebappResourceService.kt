package com.bridgething.companion

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.gateway.RequestResult
import com.bridgething.gateway.webapp
import com.bridgething.schema.TransferBody
import com.bridgething.schema.WebappResource
import com.bridgething.schema.WebappResourceKind
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

public data class WebappResourceFile(
    val file: File,
    val mime: String?,
    val sha256: String,
)

public class WebappResourceService internal constructor(
    private val cacheDir: File,
    private val gateway: BridgethingGateway,
    private val receiver: TransferReceiver,
) {
    private val dir by lazy { File(cacheDir, "bridgething-webapp-resources") }

    public suspend fun fetch(deviceId: String, webappId: UUID, kind: WebappResourceKind): WebappResourceFile? {
        val cached = newestCached(webappId, kind)
        val reply = when (val r = gateway.webapp.resource(deviceId, WebappResource(id = webappId, kind = kind, have = cached?.sha))) {
            is RequestResult.Ok -> r.response
            else -> return null
        }

        val body = reply.body
        if (body == null) {
            val hit = cached ?: return null
            return WebappResourceFile(hit.file, reply.mime ?: mimeForExt(hit.ext), reply.sha256)
        }

        val bytes = when (body) {
            is TransferBody.Inline -> body.data
            is TransferBody.Stream -> receiver.receive(deviceId, body.data)
        }
        val got = sha256Hex(bytes)
        if (got != reply.sha256.lowercase()) {
            throw IOException("webapp $webappId $kind: sha256 $got != reply ${reply.sha256}")
        }

        val dest = File(dir, "${webappId}__${kind.string}__${reply.sha256}.${extForMime(reply.mime)}")
        atomicWrite(dest, bytes)
        pruneOther(webappId, kind, keep = reply.sha256)
        return WebappResourceFile(dest, reply.mime, reply.sha256)
    }

    private data class Cached(val file: File, val sha: String, val ext: String)

    private fun newestCached(webappId: UUID, kind: WebappResourceKind): Cached? {
        val prefix = "${webappId}__${kind.string}__"
        val newest = dir.listFiles { f -> f.isFile && f.name.startsWith(prefix) }
            ?.maxByOrNull { it.lastModified() } ?: return null
        val rest = newest.name.removePrefix(prefix)
        val dot = rest.lastIndexOf('.')
        if (dot <= 0) return null
        return Cached(newest, rest.substring(0, dot), rest.substring(dot + 1))
    }

    private fun pruneOther(webappId: UUID, kind: WebappResourceKind, keep: String) {
        val prefix = "${webappId}__${kind.string}__"
        dir.listFiles { f -> f.isFile && f.name.startsWith(prefix) }?.forEach { f ->
            if (f.name.removePrefix(prefix).substringBefore('.') != keep) f.delete()
        }
    }

    private fun atomicWrite(dest: File, bytes: ByteArray) {
        dir.mkdirs()
        val tmp = File(dir, "${dest.name}.${UUID.randomUUID()}.tmp")
        tmp.writeBytes(bytes)
        runCatching {
            Files.move(tmp.toPath(), dest.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        }.onFailure {
            Files.move(tmp.toPath(), dest.toPath(), StandardCopyOption.REPLACE_EXISTING)
        }
    }

    private suspend fun sha256Hex(bytes: ByteArray): String = withContext(Dispatchers.Default) {
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
    }

    private fun extForMime(mime: String?): String =
        mime?.substringBefore(';')?.trim()?.let { EXT_BY_MIME[it] } ?: "bin"

    private fun mimeForExt(ext: String): String? = MIME_BY_EXT[ext]

    private companion object {
        val EXT_BY_MIME = mapOf(
            "image/svg+xml" to "svg",
            "image/png" to "png",
            "image/jpeg" to "jpeg",
            "image/webp" to "webp",
            "image/gif" to "gif",
            "text/html" to "html",
            "text/javascript" to "js",
        )
        val MIME_BY_EXT = EXT_BY_MIME.entries.associate { (mime, ext) -> ext to mime }
    }
}
