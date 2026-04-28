package dev.bridgething.schema

import java.util.UUID

/**
 * Wire-protocol identifiers shared by every bridgething peer (daemon + every
 * gateway language). Mirrors the Rust `pub const`s in `lib/src/lib.rs` and
 * the TS exports in `lib/ts/index.ts`. Not codegenned because typeshare
 * doesn't emit `const` items — keep these in lockstep with the Rust source by
 * hand, the values are stable wire identifiers and effectively never change.
 */
public object BridgethingProtocol {
  public val SERVICE_UUID: UUID = UUID.fromString("dead0000-53e5-4085-a5d8-f55f3f14ac5a")
  public val PROFILE_UUID: UUID = UUID.fromString("dead0000-854d-408e-81f0-fb6147f918fd")
  public val CHARACTERISTIC_UUID: UUID = UUID.fromString("dead0000-f3dc-4620-8b74-8bd49bb5a468")
  public const val RFCOMM_CHANNEL: Int = 1
  public const val MANUFACTURER_ID: Int = 0xdead
}
