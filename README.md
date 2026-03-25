# sonyctl

CLI control for Sony WH-1000XM series headphones over Bluetooth RFCOMM.

Protocol reverse-engineered from [Gadgetbridge](https://codeberg.org/Freeyourgadget/Gadgetbridge), [SonyHeadphonesClient_BE](https://github.com/BlueEve04/SonyHeadphonesClient_BE), and Bluetooth HCI snoop captures.

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

## License

MIT
