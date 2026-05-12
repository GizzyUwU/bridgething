package dev.bridgething.schema

import java.util.UUID

/**
 * Wire-protocol identifiers shared by every bridgething peer (daemon + every
 * gateway language). Mirrors the Rust `pub const`s in `crates/lib/src/lib.rs`
 * and the TS exports in `crates/lib/ts/index.ts`. Not codegenned because typeshare
 * doesn't emit `const` items - keep these in lockstep with the Rust source by
 * hand, the values are stable wire identifiers and effectively never change.
 */
public object BridgethingProtocol {
    public val PROFILE_UUID: UUID = UUID.fromString("dead0000-854d-408e-81f0-fb6147f918fd")
    public const val RFCOMM_CHANNEL: Int = 1
    public const val MANUFACTURER_ID: Int = 0xdead
}
