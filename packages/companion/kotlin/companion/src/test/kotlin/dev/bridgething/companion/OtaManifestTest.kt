package dev.bridgething.companion

import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
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
            "0.8.4+image.2026.05.0": {"version": "0.8.4+image.2026.05.0", "channel": "stable", "deprecated": false},
            "0.8.3+image.2026.04.0": {"version": "0.8.3+image.2026.04.0", "channel": "stable", "yanked": "bad build", "deprecated": false}
          }
        }
        """.trimIndent()
        val manifest = json.decodeFromString(OtaDiscoverManifest.serializer(), raw)
        assertEquals("0.8.4+image.2026.05.0", manifest.channels["stable"]?.latest)
        assertEquals(2, manifest.channels["stable"]?.releases?.size)
        assertNull(manifest.releases["0.8.4+image.2026.05.0"]?.yanked)
        assertEquals("bad build", manifest.releases["0.8.3+image.2026.04.0"]?.yanked)
    }
}
