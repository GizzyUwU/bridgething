package com.bridgething.schema

import kotlinx.serialization.json.JsonElement

/**
 * `Value` is the typeshare placeholder for `serde_json::Value` in Rust.
 * It carries the opaque payload of `ForwardMessage.Json`, which is the
 * arbitrary-data escape hatch in the bridgething wire protocol.
 *
 * Aliased to kotlinx.serialization's [JsonElement] so consumers can use
 * its existing decode/encode + DSL.
 */
typealias Value = JsonElement
