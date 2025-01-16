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
├── dev-gateway # development gateway for testing gatt code
├── gateway # sdk for creating gateway platforms
├── lib # shared communication types for rust and typescript
│   ├── src # rust types
│   └── ts # generated typescript types
├── notes # dumps/notes/etc
└── resources # random resources
```
