package com.bridgething.companion

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

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
public data class OtaArtifactDigest(
    val size: Long,
    val sha256: String,
)

/** per-artifact digests for a release, keyed to the artifacts the companion fetches by convention. */
@Serializable
public data class OtaReleaseArtifacts(
    val daemon: OtaArtifactDigest? = null,
    @SerialName("image_swu") val imageSwu: OtaArtifactDigest? = null,
    @SerialName("image_zck") val imageZck: OtaArtifactDigest? = null,
    @SerialName("image_boot_zck") val imageBootZck: OtaArtifactDigest? = null,
    val webapps: Map<String, OtaArtifactDigest> = emptyMap(),
)

@Serializable
public data class OtaManifestRelease(
    val version: String,
    val channel: String,
    val yanked: String? = null,
    val deprecated: Boolean = false,
    @SerialName("builtin_webapps") val builtinWebapps: Map<String, String> = emptyMap(),
    val artifacts: OtaReleaseArtifacts? = null,
)

public data class OtaCompositeVersion(
    val daemon: String,
    val image: String,
) {
    val composite: String get() = "$daemon+image.$image"

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

public data class OtaArtifactUrls(
    val daemonBinary: String,
    val imageSwu: String,
    val imageZck: String,
    val imageBootZck: String,
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
                imageBootZck = "$root/images/$channel/$imageVersion/$imageName-boot.zck",
            )
        }

        public fun builtinWebapp(rootUrl: String, channel: String, name: String, version: String): String {
            val root = rootUrl.trimEnd('/')
            return "$root/webapps/$channel/$name/$version/$name.zip"
        }
    }
}
