package com.bridgething.tidal

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.glue.BridgethingGlue
import com.bridgething.glue.GlueCapability
import com.bridgething.glue.GlueError
import com.bridgething.schema.MusicProvider

/**
 * Tidal glue stub. Real impl will use Tidal's OAuth + Web API. Surfaced
 * from day one so the companion app's settings UI can list it as
 * "coming soon".
 */
class TidalGlue : BridgethingGlue {
    override val name: String = "tidal"
    override val displayName: String = "Tidal"

    override val capabilities: Set<GlueCapability> = emptySet()
    override val uriSchemes: List<String> = emptyList()
    override val musicProvider: MusicProvider = MusicProvider.Tidal
    override val lyricsSupported: Boolean = false

    override suspend fun attach(gateway: BridgethingGateway) {
        throw GlueError.NotImplemented
    }

    override suspend fun detach() {}
}
