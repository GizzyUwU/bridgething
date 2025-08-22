# bridgething

This project was created to allow for Bluetooth support for the Spotify Car Thing after discontinuation date. It currently works, but only on Linux. If you connect an iPhone to bridgething on a Car Thing running the stock UI, it will allow basic playback control. The gateways are the only things not implemented.

I no longer have the time to work on bridgething. I would love it if someone used this project to complete the restoration of the Spotify Car Thing.

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

## Notes

For development, the bridgething host device needs to have the bluetooth class `0x7c0000`. This can be set by running `sudo hciconfig hci0 class 0x7c0000` (where `hci0` is the bluetooth adapter).

BridgeThing expects a `chromium` instance with `--remote-debugging-port=9222` to be running. You don't need it running, but BridgeThing will be a little confused without it.
