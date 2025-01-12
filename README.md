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
├── client # sdks for creating client applications
│   ├── rust # rust client sdk
│   └── typescript # typescript client sdk
├── core # primary bridgething daemon
├── dev # development tools
│   ├── adapter # development gatt adapter for bluez
│   └── gateway # development gateway for testing gatt code
├── gateway # sdk for creating gateway platforms
├── notes # dumps/notes/etc
└── resources # random resources
```
