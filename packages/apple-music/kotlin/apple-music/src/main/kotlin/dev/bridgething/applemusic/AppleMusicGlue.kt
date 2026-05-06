package dev.bridgething.applemusic

import dev.bridgething.gateway.BridgethingGateway
import dev.bridgething.glue.BridgethingGlue
import dev.bridgething.glue.GlueCapability
import dev.bridgething.glue.GlueError

/**
 * Apple Music glue stub. Real impl will use the Apple Music Web API
 * with the user's signed-in subscription token. Surfaced from day one
 * so the companion app's settings UI can list it as "coming soon".
 */
class AppleMusicGlue : BridgethingGlue {
    override val name: String = "apple-music"
    override val displayName: String = "Apple Music"

    override val capabilities: Set<GlueCapability> = emptySet()

    override suspend fun attach(gateway: BridgethingGateway) {
        throw GlueError.NotImplemented
    }

    override suspend fun detach() {}
}
