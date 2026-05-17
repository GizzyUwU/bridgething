package dev.bridgething.companion

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Subset of the thinglabs discover-manifest schema the poll loop reads
 * when picking an OTA release. Mirror of Swift `OtaDiscoverManifest`;
 * the site-side validator owns the full schema.
 */
@Serializable
public data class OtaDiscoverManifest(
    @SerialName("manifest_version") val manifestVersion: Int,
    @SerialName("updated_at") val updatedAt: String,
    val channels: Map<String, OtaManifestChannel>,
    val releases: Map<String, OtaManifestRelease>,
)

@Serializable
public data class OtaManifestChannel(
    val name: String,
    val stability: String,
    @SerialName("default") val isDefault: Boolean,
    val latest: String,
    val releases: List<String>,
)

@Serializable
public data class OtaManifestRelease(
    val version: String,
    val channel: String,
    val yanked: String? = null,
    val deprecated: Boolean = false,
)

/**
 * Composite version parsed out of a channel's `latest`. The bridgething
 * release pipeline uses `<daemon>+image.<image>` (daemon as semver,
 * image as CalVer) so the companion compares each component
 * independently to the device's announced versions.
 */
public data class OtaCompositeVersion(
    val daemon: String,
    val image: String,
) {
    public companion object {
        public fun parse(raw: String): OtaCompositeVersion? {
            val plus = raw.indexOf('+').takeIf { it >= 0 } ?: return null
            val daemon = raw.substring(0, plus)
            val suffix = raw.substring(plus + 1)
            val prefix = "image."
            if (!suffix.startsWith(prefix)) return null
            val image = suffix.substring(prefix.length)
            if (daemon.isEmpty() || image.isEmpty()) return null
            return OtaCompositeVersion(daemon = daemon, image = image)
        }
    }
}

/**
 * Per-artifact URLs derived from the OTA root + channel + per-component
 * version + image variant. Mirror of Swift `OtaArtifactURLs`; matches the
 * on-disk R2 layout documented in `notes/release-pipeline.md`.
 */
public data class OtaArtifactUrls(
    val daemonBinary: String,
    val imageSwu: String,
    val imageZck: String,
) {
    public companion object {
        public fun build(
            rootUrl: String,
            channel: String,
            daemonVersion: String,
            imageVersion: String,
            imageVariant: String,
        ): OtaArtifactUrls {
            val root = rootUrl.trimEnd('/')
            val imageName = "bridgething-$imageVariant-image"
            return OtaArtifactUrls(
                daemonBinary = "$root/daemon/$channel/$daemonVersion/bridgething",
                imageSwu = "$root/images/$channel/$imageVersion/$imageName.swu",
                imageZck = "$root/images/$channel/$imageVersion/$imageName.zck",
            )
        }
    }
}
