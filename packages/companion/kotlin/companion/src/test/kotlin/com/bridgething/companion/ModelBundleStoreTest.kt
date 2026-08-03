package com.bridgething.companion

import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

private class FakeTransport(
    val manifests: () -> ModelManifest,
    val artifacts: (String) -> File,
    val gate: CompletableDeferred<Unit>? = null,
    val started: CompletableDeferred<Unit>? = null,
) : ModelBundleTransport {
    val downloads = AtomicInteger(0)

    override suspend fun manifest(url: String): ModelManifest = manifests()

    override suspend fun download(
        artifact: ModelArtifact,
        dir: File,
        onProgress: suspend (Long, Long) -> Unit,
    ): File {
        downloads.incrementAndGet()
        started?.complete(Unit)
        gate?.await()
        dir.mkdirs()
        val source = artifacts(artifact.sha256)
        val dest = File(dir, "download-${artifact.sha256}-${source.name}")
        source.copyTo(dest, overwrite = true)
        onProgress(artifact.size, artifact.size)
        return dest
    }
}

private fun scratch(): File =
    File(System.getProperty("java.io.tmpdir"), "model-store-test-${UUID.randomUUID()}").also { it.mkdirs() }

private fun makeBundleZip(
    dir: File,
    name: String,
    entries: List<String> = listOf("manifest.json", "tokenizer.json", "model.tflite"),
): File {
    val zip = File(dir, "$name.zip")
    ZipOutputStream(zip.outputStream().buffered()).use { out ->
        for (entry in entries) {
            out.putNextEntry(ZipEntry(entry))
            out.write("{}".toByteArray())
            out.closeEntry()
        }
    }
    return zip
}

private fun makeWeights(dir: File, name: String, bytes: ByteArray = "ggml weights".toByteArray()): File =
    File(dir, "$name.bin").also { it.writeBytes(bytes) }

private fun decodeManifest(body: String): ModelManifest =
    ModelBundleStore.defaultJson.decodeFromString(ModelManifest.serializer(), body)

private fun manifestJson(version: String, sha: String): ModelManifest = decodeManifest(
    """
    {
      "version": "$version",
      "updated_at": "2026-08-02T00:00:00Z",
      "ios": {
        "url": "https://ota.bridgething.com/nlu/stable/bundle/$version/bundle-ios.zip",
        "size": 1024,
        "sha256": "ios-$sha"
      },
      "android": {
        "url": "https://ota.bridgething.com/nlu/stable/bundle/$version/bundle-android.zip",
        "size": 512,
        "sha256": "$sha"
      }
    }
    """.trimIndent(),
)

private fun asrManifestJson(version: String, sha: String): ModelManifest = decodeManifest(
    """
    {
      "version": "$version",
      "model": "tiny.en",
      "updated_at": "2026-08-02T00:00:00Z",
      "android": {
        "url": "https://ota.bridgething.com/asr/stable/model/$version/ggml-tiny.en.bin",
        "size": 512,
        "sha256": "$sha"
      }
    }
    """.trimIndent(),
)

private fun store(
    dir: File,
    transport: ModelBundleTransport,
    kind: ModelBundleKind = ModelBundleKind.Nlu,
    enabled: Boolean = true,
    policy: ModelTransferPolicy = ModelTransferPolicy { true },
    validator: suspend (File) -> Unit = {},
): ModelBundleStore = ModelBundleStore(
    kind = kind,
    config = ModelBundleStore.Config(storageDirectory = dir),
    enabled = enabled,
    transport = transport,
    transferPolicy = policy,
    validator = validator,
)

class ModelBundleStoreTest {
    @Test
    fun `the manifest decodes the published shape and picks the android artifact`() {
        val manifest = manifestJson("1.0.0", "aaa")
        assertEquals("1.0.0", manifest.version)
        assertEquals("2026-08-02T00:00:00Z", manifest.updatedAt)
        assertEquals(512L, manifest.android?.size)
        assertEquals("aaa", manifest.android?.sha256)
        assertEquals(ArtifactDigest(512L, "aaa"), manifest.android?.digest)
    }

    @Test
    fun `a fresh bundle validates and rotates into place`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val store = store(dir, FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip }))

        store.ensure()

        assertEquals(ModelBundleState.Ready("1.0.0"), store.state)
        val live = requireNotNull(store.live)
        assertTrue(File(live, "manifest.json").exists())
        assertTrue(File(live, "model.tflite").exists())
    }

    @Test
    fun `state changes are observable on the flow`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val store = store(dir, FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip }))
        val ready = async { store.states.first { it is ModelBundleState.Ready } }

        store.ensure()

        assertEquals(ModelBundleState.Ready("1.0.0"), withTimeout(5_000) { ready.await() })
    }

    @Test
    fun `an already-installed version is not downloaded again`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val transport = FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip })
        val store = store(dir, transport)

        store.ensure()
        store.ensure()

        assertEquals(1, transport.downloads.get())
    }

    @Test
    fun `a bundle that fails validation leaves the previous one serving`() = runBlocking {
        val dir = scratch()
        val first = makeBundleZip(dir, "v1")
        val second = makeBundleZip(dir, "v2")
        var version = "1.0.0"
        val store = store(
            dir,
            FakeTransport({ manifestJson(version, version) }, { sha -> if (sha == "1.0.0") first else second }),
            validator = { if (version != "1.0.0") throw ModelBundleException("model refused to load") },
        )

        store.ensure()
        assertEquals(ModelBundleState.Ready("1.0.0"), store.state)

        version = "2.0.0"
        store.ensure()

        assertEquals(ModelBundleState.Ready("1.0.0"), store.state)
        assertEquals("1.0.0", store.live?.name)
        assertFalse(File(dir, "bridgething-nlu/2.0.0").exists())
    }

    @Test
    fun `an archive missing a required entry never rotates in`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1", entries = listOf("manifest.json", "tokenizer.json"))
        val store = store(dir, FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip }))

        store.ensure()

        assertNull(store.live)
        val failed = store.state as? ModelBundleState.Failed
        assertEquals("nlu archive is missing model.tflite", failed?.reason)
        assertFalse(File(dir, "bridgething-nlu/1.0.0").exists())
    }

    @Test
    fun `an archive entry escaping the staging root is rejected`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(
            dir,
            "evil",
            entries = listOf("manifest.json", "tokenizer.json", "model.tflite", "../escaped.txt"),
        )
        val store = store(dir, FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip }))

        store.ensure()

        val failed = store.state as? ModelBundleState.Failed
        assertEquals("nlu archive entry escapes the staging root: ../escaped.txt", failed?.reason)
        assertNull(store.live)
        assertFalse(File(dir, "bridgething-nlu/escaped.txt").exists())
    }

    @Test
    fun `turning the capability off deletes the stored bundle`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val store = store(dir, FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip }))
        store.ensure()

        store.setEnabled(false)

        assertEquals(ModelBundleState.Absent, store.state)
        assertNull(store.live)
        assertFalse(File(dir, "bridgething-nlu").exists())
    }

    @Test
    fun `a disabled store never checks`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val transport = FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip })
        val store = store(dir, transport, enabled = false)

        store.ensure()

        assertEquals(0, transport.downloads.get())
        assertEquals(ModelBundleState.Absent, store.state)
    }

    @Test
    fun `rotating a new version prunes the one it replaced`() = runBlocking {
        val dir = scratch()
        val first = makeBundleZip(dir, "v1")
        val second = makeBundleZip(dir, "v2")
        var version = "1.0.0"
        val store = store(
            dir,
            FakeTransport({ manifestJson(version, version) }, { sha -> if (sha == "1.0.0") first else second }),
        )

        store.ensure()
        version = "2.0.0"
        store.ensure()

        assertEquals(ModelBundleState.Ready("2.0.0"), store.state)
        val entries = File(dir, "bridgething-nlu").list()?.toSortedSet()
        assertEquals(sortedSetOf("2.0.0", "current"), entries)
    }

    @Test
    fun `a new store adopts a bundle already on disk`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val transport = FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip })
        store(dir, transport).ensure()

        val second = store(dir, transport)

        assertEquals(ModelBundleState.Ready("1.0.0"), second.state)
        assertNotNull(second.live)
    }

    @Test
    fun `overlapping ensure calls share a single download`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val gate = CompletableDeferred<Unit>()
        val started = CompletableDeferred<Unit>()
        val transport = FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip }, gate = gate, started = started)
        val store = store(dir, transport)

        coroutineScope {
            val first = launch { store.ensure() }
            started.await()
            val second = launch { store.ensure() }
            gate.complete(Unit)
            first.join()
            second.join()
        }

        assertEquals(1, transport.downloads.get())
        assertEquals(ModelBundleState.Ready("1.0.0"), store.state)
    }

    @Test
    fun `a metered network defers the download`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val transport = FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip })
        val store = store(dir, transport, policy = { false })

        store.ensure()

        assertEquals(0, transport.downloads.get())
        assertEquals(ModelBundleState.Absent, store.state)
    }
}

class AsrModelStoreTest {
    @Test
    fun `the asr manifest decodes without the fields only the nlu one carries`() {
        val manifest = asrManifestJson("1.0.0", "aaa")
        assertEquals("1.0.0", manifest.version)
        assertNull(manifest.ios)
        assertEquals(ArtifactDigest(512L, "aaa"), manifest.android?.digest)
    }

    @Test
    fun `a bare weights file installs and resolves to the file itself`() = runBlocking {
        val dir = scratch()
        val weights = makeWeights(dir, "v1")
        val store = store(dir, FakeTransport({ asrManifestJson("1.0.0", "aaa") }, { weights }), kind = ModelBundleKind.Asr)

        store.ensure()

        assertEquals(ModelBundleState.Ready("1.0.0"), store.state)
        val live = requireNotNull(store.live)
        assertTrue(live.isFile)
        assertEquals("model.bin", live.name)
        assertEquals("1.0.0", live.parentFile.name)
        assertEquals("ggml weights", live.readText())
    }

    @Test
    fun `the validator sees the weights file, not its directory`() = runBlocking {
        val dir = scratch()
        val weights = makeWeights(dir, "v1")
        var seen: String? = null
        val store = store(
            dir,
            FakeTransport({ asrManifestJson("1.0.0", "aaa") }, { weights }),
            kind = ModelBundleKind.Asr,
            validator = { seen = if (it.isFile) it.name else "not a file: ${it.name}" },
        )

        store.ensure()

        assertEquals("model.bin", seen)
    }

    @Test
    fun `a model the validator rejects never rotates in`() = runBlocking {
        val dir = scratch()
        val weights = makeWeights(dir, "v1", bytes = "not a ggml file".toByteArray())
        val store = store(
            dir,
            FakeTransport({ asrManifestJson("1.0.0", "aaa") }, { weights }),
            kind = ModelBundleKind.Asr,
            validator = { throw ModelBundleException("asr model header is not ggml") },
        )

        store.ensure()

        assertNull(store.live)
        assertEquals("asr model header is not ggml", (store.state as? ModelBundleState.Failed)?.reason)
        assertFalse(File(dir, "bridgething-asr/1.0.0").exists())
    }

    @Test
    fun `an empty download never rotates in`() = runBlocking {
        val dir = scratch()
        val weights = makeWeights(dir, "v1", bytes = ByteArray(0))
        val store = store(dir, FakeTransport({ asrManifestJson("1.0.0", "aaa") }, { weights }), kind = ModelBundleKind.Asr)

        store.ensure()

        assertNull(store.live)
        assertEquals("asr model is missing model.bin", (store.state as? ModelBundleState.Failed)?.reason)
    }

    @Test
    fun `rotating a new model prunes the one it replaced`() = runBlocking {
        val dir = scratch()
        val first = makeWeights(dir, "v1")
        val second = makeWeights(dir, "v2", bytes = "ggml newer".toByteArray())
        var version = "1.0.0"
        val store = store(
            dir,
            FakeTransport({ asrManifestJson(version, version) }, { sha -> if (sha == "1.0.0") first else second }),
            kind = ModelBundleKind.Asr,
        )

        store.ensure()
        version = "2.0.0"
        store.ensure()

        assertEquals(ModelBundleState.Ready("2.0.0"), store.state)
        assertEquals("ggml newer", store.live?.readText())
        assertEquals(sortedSetOf("2.0.0", "current"), File(dir, "bridgething-asr").list()?.toSortedSet())
    }

    @Test
    fun `a new store adopts a model already on disk`() = runBlocking {
        val dir = scratch()
        val weights = makeWeights(dir, "v1")
        val transport = FakeTransport({ asrManifestJson("1.0.0", "aaa") }, { weights })
        store(dir, transport, kind = ModelBundleKind.Asr).ensure()

        val second = store(dir, transport, kind = ModelBundleKind.Asr)

        assertEquals(ModelBundleState.Ready("1.0.0"), second.state)
        assertNotNull(second.live)
    }

    @Test
    fun `the two stores install side by side under their own roots`() = runBlocking {
        val dir = scratch()
        val zip = makeBundleZip(dir, "v1")
        val weights = makeWeights(dir, "v1")

        store(dir, FakeTransport({ manifestJson("1.0.0", "aaa") }, { zip })).ensure()
        store(dir, FakeTransport({ asrManifestJson("2.0.0", "bbb") }, { weights }), kind = ModelBundleKind.Asr).ensure()

        assertTrue(File(dir, "bridgething-nlu/1.0.0/model.tflite").exists())
        assertTrue(File(dir, "bridgething-asr/2.0.0/model.bin").exists())
    }

    @Test
    fun `turning the capability off deletes the stored model`() = runBlocking {
        val dir = scratch()
        val weights = makeWeights(dir, "v1")
        val store = store(dir, FakeTransport({ asrManifestJson("1.0.0", "aaa") }, { weights }), kind = ModelBundleKind.Asr)
        store.ensure()

        store.setEnabled(false)

        assertEquals(ModelBundleState.Absent, store.state)
        assertFalse(File(dir, "bridgething-asr").exists())
    }
}
