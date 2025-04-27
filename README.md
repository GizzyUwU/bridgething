# bridgething

coming soon :)

this code needs some serious refactoring lol - dw i am well aware

## Data Flow Diagram

```none
      "client"
┌──────────────────┐
│                  │
│    native app/   │
│      webapp/     │
│   anything else  │
│                  │
└───────▲───┬──────┘
      websocket
┌───────┴───▼──────┐           ┌──────────────────┐
│                  │           │                  │
│    bridgething   ┼───────────►      phone/      │
│      daemon      │ bluetooth │    deskthing     │
│                  ◄───────────┼                  │
└──────────────────┘           └──────────────────┘
      "daemon"                      "gateway"
```

## Project Structure

```bash
.
├── adapter # cross-platform bluetooth gatt adapter for use in gateways
├── client # sdks for creating client applications
│   ├── rust # rust sdk
│   └── typescript # typescript sdk
├── core # primary bridgething daemon
├── gateway # sdk for creating gateway platforms
├── lib # shared communication types for rust and typescript
│   ├── src # rust types
│   └── ts # generated typescript types
├── notes # dumps/notes/etc
└── resources # random resources
```

## Notes

For development, the bridgething host device needs to have the bluetooth class `0x7c0000`. This can be set by running `sudo hciconfig hci0 class 0x7c0000` (where `hci0` is the bluetooth adapter).

BridgeThing expects a `chromium` instance with `--remote-debugging-port=9222` to be running. You don't need it running, but BridgeThing will be a little confused without it.
