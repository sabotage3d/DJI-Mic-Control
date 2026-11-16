# DJI Mic Series Mobile Receiver: RF-link investigation

This project has a **read-only wireless link diagnostic** for the mobile receiver with USB VID:PID `2ca3:4011`. It does not expose a receiver power switch, send undocumented RF control commands, or change transmitter pairing.

## What is verified

- USB interface 6, bulk endpoints 0x06/0x86 carry the vendor protocol.
- On v2 status pushes, receiver frame offset **44** reports a bitmask: bit 0 = TX1 connected, bit 1 = TX2 connected. The app already decodes this into `DeviceStatus.tx`.
- `Wireless link (read-only)` in the KDE system tray shows one of: unknown, no transmitters connected, TX1, TX2, or both.
- Changes print `[DJI RF read-only] ... -> ...` to stderr. This uses the existing DeviceManager; it does not open a second USB connection.

## Safe validation on Arch Linux

```bash
cd ~/Downloads/DJI-Mic-Control
git pull --ff-only
(cd gui && npm run build)
cargo test -p djimic-gui
cargo build --release -p djimic-gui --features custom-protocol
pkill -x djimic-gui 2>/dev/null || true
./target/release/djimic-gui 2>&1 | tee /tmp/dji-rf-link.log
```

While **not on a call**, observe `Wireless link (read-only)` in the tray:

1. With the TX powered on, it should say `TX1 connected` (or TX2).
2. **Power down the TX using its own power button** (not the pairing/link button); confirm that the tray becomes `No transmitters connected`.
3. Power on the TX again and confirm that it rejoins and the status changes back to connected.
4. Review `grep -F '[DJI RF read-only]' /tmp/dji-rf-link.log`.

The test confirms that the receiver status decoder sees real link transitions. **It does not establish a software command to trigger those transitions.**

## Remaining protocol problem

The documented setting command set includes gain (`0x0039`), 15-minute auto-off configuration (`0x0010`), and audio settings, but no verified **RF off / RF on** or reversible transmitter standby command.

**Do not send random command IDs.** Some HID commands reset the receiver or enter firmware-update mode. Unpair/disconnect operations that delete remembered TX associations would also defeat automatic wake/reconnect.

Next research targets: a vendor-confirmed reversible link-control API, or captured USB traffic from a known safe RF standby/resume operation. Only after a specific, verified reversible command is identified should the GUI/tray expose an ON/OFF action. Automatic idle sleep remains disabled.
