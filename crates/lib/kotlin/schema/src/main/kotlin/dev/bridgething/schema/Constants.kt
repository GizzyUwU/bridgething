package dev.bridgething.schema

import java.util.UUID

/**
 * Wire-protocol identifiers shared by every bridgething peer (daemon + every
 * gateway language). Not codegenned because typeshare doesn't emit `const`
 * items, so these are kept in lockstep with the Rust constants by hand; the
 * values are stable wire identifiers and effectively never change.
 */
public object BridgethingProtocol {
    public val PROFILE_UUID: UUID = UUID.fromString("dead0000-854d-408e-81f0-fb6147f918fd")
    public const val RFCOMM_CHANNEL: Int = 1
    public const val MANUFACTURER_ID: Int = 0xdead
}
