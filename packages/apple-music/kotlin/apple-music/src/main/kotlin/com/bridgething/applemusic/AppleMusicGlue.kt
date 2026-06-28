package com.bridgething.applemusic

import com.bridgething.gateway.BridgethingGateway
import com.bridgething.glue.BridgethingGlue
import com.bridgething.glue.GlueCapability
import com.bridgething.glue.GlueError
import com.bridgething.schema.MusicProvider

/**
 * Apple Music glue stub. Real impl will use the Apple Music Web API
 * with the user's signed-in subscription token. Surfaced from day one
 * so the companion app's settings UI can list it as "coming soon".
 */
class AppleMusicGlue : BridgethingGlue {
    override val name: String = "apple-music"
    override val displayName: String = "Apple Music"

    override val capabilities: Set<GlueCapability> = emptySet()
    override val uriSchemes: List<String> = emptyList()
    override val musicProvider: MusicProvider = MusicProvider.AppleMusic
    override val lyricsSupported: Boolean = false

    override suspend fun attach(gateway: BridgethingGateway) {
        throw GlueError.NotImplemented
    }

    override suspend fun detach() {}
}
