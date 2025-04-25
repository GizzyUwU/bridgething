# bridgething bluetooth rfcomm protocol

This document describes the binary protocol used for communication over Bluetooth RFCOMM in bridgething.

## Packet Structure

Each message sent over the RFCOMM channel uses the following structure:

| Field            | Size (bytes) | Description                             |
| ---------------- | ------------ | --------------------------------------- |
| Magic Number     | 2            | Fixed value: `0xdead` (big-endian)      |
| Version          | 1            | Protocol version (currently 1)          |
| Compression Type | 1            | Compression algorithm (see below)       |
| Length           | 8            | Length of the following JSON data (u64) |
| JSON Data        | variable     | Message payload, possibly compressed    |

### Field Details

- **Magic Number**: Always `0xdead` (2 bytes, big-endian). Used to identify the start of a valid packet.
- **Version**: Protocol version. The current version is `1`.
- **Compression Type**: Indicates the compression algorithm used for the payload. The following values are supported:
  - `0x00`: no compression
  - `0x01`: gzip
- **Length**: 8-byte unsigned integer (big-endian) specifying the length in bytes of the following JSON data (after compression, if any).
- **JSON Data**: The actual message payload, encoded as JSON. If compression is used, this field contains the compressed data.

## Example

A valid packet might look like:

```none
[0xDE, 0xAD, 0x01, 0x00, <8-byte length>, <json bytes>]
```

or, with gzip compression:

```none
[0xDE, 0xAD, 0x01, 0x01, <8-byte length>, <gzip-compressed json bytes>]
```

## Notes

- Only no compression (`0x00`) and gzip compression (`0x01`) are currently supported. Other values are reserved for future use.
- All multi-byte fields are encoded in big-endian order.
- The JSON data should be a valid JSON-encoded object or array, as defined by your application.
- Gzipped JSON is the preferred format as it provides better compatibility than MessagePack.

## Version Matrix

| Version  | Features / Changes                                 | Compatibility           |
| -------- | -------------------------------------------------- | ----------------------- |
| 1        | Initial version. Supports no compression and gzip. | Backward compatible     |
| (future) | (Add new features/fields as protocol evolves)      | (Specify compatibility) |

**Notes:**

- This table will be updated as new protocol versions are introduced.
- Compatibility column indicates if the version is backward compatible with previous versions.
