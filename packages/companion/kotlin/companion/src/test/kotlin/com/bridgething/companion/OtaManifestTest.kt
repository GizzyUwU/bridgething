package com.bridgething.companion

import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class OtaManifestTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `composite version parse`() {
        val v = OtaCompositeVersion.parse("0.8.4+image.2026.05.0")
        assertEquals("0.8.4", v?.daemon)
        assertEquals("2026.05.0", v?.image)

        assertNull(OtaCompositeVersion.parse("0.8.4"))
        assertNull(OtaCompositeVersion.parse("0.8.4+2026.05.0"))
        assertNull(OtaCompositeVersion.parse("+image.2026.05.0"))
        assertNull(OtaCompositeVersion.parse("0.8.4+image."))
    }

    @Test
    fun `artifact urls`() {
        val urls = OtaArtifactUrls.build(
            rootUrl = "https://ota.bridgething.com",
            channel = "stable",
            daemonVersion = "0.8.4",
            imageVersion = "2026.05.0",
            imageVariant = "prod",
        )
        assertEquals("https://ota.bridgething.com/daemon/stable/0.8.4/bridgething", urls.daemonBinary)
        assertEquals("https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image.swu", urls.imageSwu)
        assertEquals("https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image.zck", urls.imageZck)
        assertEquals(
            "https://ota.bridgething.com/images/stable/2026.05.0/bridgething-prod-image-boot.zck",
            urls.imageBootZck,
        )

        assertEquals(
            "https://ota.bridgething.com/webapps/stable/hub/0.1.0/hub.zip",
            OtaArtifactUrls.builtinWebapp("https://ota.bridgething.com", "stable", "hub", "0.1.0"),
        )
    }

    @Test
    fun `manifest decode`() {
        val raw = """
        {
          "manifest_version": 1,
          "updated_at": "2026-05-30T00:00:00Z",
          "channels": {
            "stable": {
              "name": "stable", "stability": "stable", "default": true,
              "latest": "0.8.4+image.2026.05.0",
              "releases": ["0.8.4+image.2026.05.0", "0.8.3+image.2026.04.0"]
            }
          },
          "releases": {
            "0.8.4+image.2026.05.0": {"version": "0.8.4+image.2026.05.0", "channel": "stable", "deprecated": false, "builtin_webapps": {"hub": "0.1.0", "stock": "8.9.2"}},
            "0.8.3+image.2026.04.0": {"version": "0.8.3+image.2026.04.0", "channel": "stable", "yanked": "bad build", "deprecated": false}
          }
        }
        """.trimIndent()
        val manifest = json.decodeFromString(OtaDiscoverManifest.serializer(), raw)
        assertEquals("0.8.4+image.2026.05.0", manifest.channels["stable"]?.latest)
        assertEquals(2, manifest.channels["stable"]?.releases?.size)
        assertNull(manifest.releases["0.8.4+image.2026.05.0"]?.yanked)
        assertEquals("bad build", manifest.releases["0.8.3+image.2026.04.0"]?.yanked)
        assertEquals(
            mapOf("hub" to "0.1.0", "stock" to "8.9.2"),
            manifest.releases["0.8.4+image.2026.05.0"]?.builtinWebapps,
        )
        assertEquals(emptyMap<String, String>(), manifest.releases["0.8.3+image.2026.04.0"]?.builtinWebapps)
    }

    @Test
    fun `daemon patch digest decodes source sha256`() {
        val raw = """
        {
          "daemon": {"size": 100, "sha256": "aa"},
          "daemon_patches": {
            "0.8.3": {"size": 10, "sha256": "bb", "source_sha256": "cc"},
            "0.8.2": {"size": 20, "sha256": "dd"}
          }
        }
        """.trimIndent()
        val artifacts = json.decodeFromString(OtaReleaseArtifacts.serializer(), raw)
        assertEquals("cc", artifacts.daemonPatches["0.8.3"]?.sourceSha256)
        assertEquals(OtaArtifactDigest(10, "bb"), artifacts.daemonPatches["0.8.3"]?.digest)
        assertNull(artifacts.daemonPatches["0.8.2"]?.sourceSha256)
    }

    @Test
    fun `patch source match gate`() {
        assertTrue(patchSourceMatches("abc", "abc"))
        assertTrue(patchSourceMatches("ABC", "abc"))
        assertFalse(patchSourceMatches("abc", "def"))
        assertTrue(patchSourceMatches(null, "abc"))
        assertTrue(patchSourceMatches("abc", null))
        assertTrue(patchSourceMatches(null, null))
    }
}
