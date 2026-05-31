package dev.bridgething.companion

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * A `catalog.v1` app catalog as served at a source URL. The companion decodes
 * the fields it needs to browse, install, and update webapps; the site-side
 * validator (`bridgething.com/apps`) owns the full schema and its invariants.
 */
@Serializable
public data class Catalog(
    val schema: String,
    @SerialName("updated_at") val updatedAt: String,
    val repo: CatalogRepo,
    val apps: List<CatalogApp>,
    @SerialName("recommended_sources") val recommendedSources: List<CatalogRecommendedSource> = emptyList(),
)

@Serializable
public data class CatalogRepo(
    val name: String,
    val description: String,
    val homepage: String? = null,
    val icon: String? = null,
)

@Serializable
public data class CatalogApp(
    /** uuidv7 baked into the webapp; the stable identity that keys upgrade-in-place and the device KV namespace. */
    val id: String,
    val name: String,
    val description: String,
    val author: String,
    val icon: String? = null,
    val homepage: String? = null,
    val source: String? = null,
    /** Newest-first, per the catalog's ordering invariant. */
    val versions: List<CatalogAppVersion>,
)

@Serializable
public data class CatalogAppVersion(
    val version: String,
    @SerialName("released_at") val releasedAt: String,
    val download: CatalogDownload,
    /** Permission catalog keys this version requests. Shown at install, informational; never a gate. */
    val permissions: List<String>,
    /** Dotted semver, no leading `v`. The device's announced libbridgethingVersion is v-prefixed; comparison strips it. */
    @SerialName("min_libbridgething_version") val minLibbridgethingVersion: String,
    val changelog: String? = null,
)

@Serializable
public data class CatalogDownload(
    val url: String,
    val size: Long,
    val sha256: String,
)

@Serializable
public data class CatalogRecommendedSource(
    val name: String,
    val url: String,
    val description: String? = null,
    val attested: Boolean,
)

/**
 * Dotted-semver comparison for the `min_libbridgething_version` compat gate. A
 * leading `v` (the device announces `v12.0.1`) and any pre-release/build suffix
 * are stripped; missing components count as zero.
 */
public object SemverCompat {
    /** True when [deviceVersion] is at least [minimum]. */
    public fun satisfies(deviceVersion: String, minimum: String): Boolean =
        compare(deviceVersion, minimum) >= 0

    public fun compare(a: String, b: String): Int {
        val pa = components(a)
        val pb = components(b)
        for (i in 0 until maxOf(pa.size, pb.size)) {
            val x = pa.getOrElse(i) { 0 }
            val y = pb.getOrElse(i) { 0 }
            if (x != y) return if (x < y) -1 else 1
        }
        return 0
    }

    private fun components(s: String): List<Int> {
        var v = s
        if (v.startsWith("v") || v.startsWith("V")) v = v.substring(1)
        val cut = v.indexOfFirst { it == '-' || it == '+' }
        if (cut >= 0) v = v.substring(0, cut)
        return v.split(".").map { it.toIntOrNull() ?: 0 }
    }
}
