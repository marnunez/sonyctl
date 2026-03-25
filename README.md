# sonyctl

CLI control for Sony WH-1000XM series headphones over Bluetooth RFCOMM. Heavily vivecoded.

Protocol reverse-engineered from [Gadgetbridge](https://codeberg.org/Freeyourgadget/Gadgetbridge), [SonyHeadphonesClient](https://github.com/Plutoberth/SonyHeadphonesClient) ([BE fork](https://github.com/BlueEve04/SonyHeadphonesClient_BE)), [ohm-app protocol docs](https://github.com/ohm-app/sony-headphones-bluetooth-documentation), and Bluetooth HCI snoop captures. No code was copied — only protocol details were studied. See [Acknowledgements](#acknowledgements).

## Features

```
sonyctl status                    # Full dashboard
sonyctl info                      # Model, firmware, active codec
sonyctl battery                   # Battery percentage
sonyctl anc                       # Enable noise cancelling
sonyctl ambient [1-20] [--voice]  # Ambient sound mode
sonyctl anc-off                   # Disable ANC / ambient
sonyctl dsee [on|off]             # DSEE upsampling
sonyctl auto-off [0|5|30|60|180]  # Auto power-off (minutes)
sonyctl voice-guidance [on|off]   # Voice guidance notifications
sonyctl eq get|flat|set           # 10-band equalizer
sonyctl volume [0-30]             # Volume control
sonyctl stc [on|off]              # Speak-to-Chat
sonyctl play|pause|next|prev      # Media controls
sonyctl devices                   # Connected + paired devices
sonyctl multipoint [on|off]       # Multipoint status/toggle
sonyctl power-off                 # Shutdown headphones
```

## Install

```bash
cargo install --path .
```

Or with Nix:

```bash
nix run github:marnunez/sonyctl
```

## Requirements

- Linux with BlueZ
- Headphones paired and connected via Bluetooth
- No root required — uses standard RFCOMM sockets

## Tested devices

- **WH-1000XM6** (primary target, V2 protocol)

Should work with WH-1000XM5 and WF-1000XM5 (V2 protocol). Older XM3/XM4 (V1) may need adjustments.

## How it works

Communicates over Bluetooth RFCOMM using the same binary protocol as the Sony Sound Connect app. Auto-detects connected Sony headphones via `bluetoothctl`. No dependencies beyond `libc`.

## Acknowledgements

This project would not be possible without the reverse-engineering work of these projects:

- **[SonyHeadphonesClient](https://github.com/Plutoberth/SonyHeadphonesClient)** (MIT) — original protocol reverse-engineering by Plutoberth for WH-1000XM3
- **[SonyHeadphonesClient_BE](https://github.com/BlueEve04/SonyHeadphonesClient_BE)** (MIT) — XM5+ protocol updates by mos9527/BlueEve04
- **[Gadgetbridge](https://codeberg.org/Freeyourgadget/Gadgetbridge)** (AGPL-3.0) — V1/V2 protocol implementations and the critical ACK sequence number fix ([PR #2456](https://codeberg.org/Freeyourgadget/Gadgetbridge/pulls/2456))
- **[sony-headphones-bluetooth-documentation](https://github.com/ohm-app/sony-headphones-bluetooth-documentation)** (CC0) — protocol specification docs

No code was copied from any of these projects. Only protocol details (command opcodes, packet framing, byte layouts) were studied and independently reimplemented.

## License

MIT
