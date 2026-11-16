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


## Protocol research: what has actually been found

The search covered the current USB implementations in [DJI-Mic-Control](https://github.com/ShadowBitBasher/DJI-Mic-Control), [lavdeck](https://github.com/piedol/lavdeck), [dji-mic-mo](https://github.com/usokawa/dji-mic-mo), and [dji-mic-mini-kde](https://github.com/russellmilliner/dji-mic-mini-kde).

- The documented v2 command 0x0039 adjusts receiver gain. Command 0x0010 configures the 15-minute automatic shutdown feature; it is **not** an immediate wireless sleep command.
- No tested, reversible **RF disable / RF enable** command is documented for USB device 2ca3:4011.
- An HID 0x52 command is described inconsistently by projects as reboot or power-off. **Do not send it**: neither description establishes reversible RF-only standby, and it may lose the USB control path.
- DJI says Mic Mini TX enters power saving when RX signal disappears, and reconnects when RX signal returns; this documents the desired end behavior, not the existence of a host USB command to induce it.
- Pausing USB audio, changing PipeWire inputs or disabling a USB *data* port does **not** prove the proprietary TX/RX radio link has stopped.

DJI FAQ: https://www.dji.com/mic-mini/faq

### Additional read-only transport trace (Stage 3.1)

Run:

\`\`\`bash
cd ~/Downloads/DJI-Mic-Control
git pull --ff-only
cargo test -p device
cargo build --release -p djimic-gui --features custom-protocol
pkill -x djimic-gui 2>/dev/null || true
DJI_RF_TRACE=1 ./target/release/djimic-gui 2>&1 | tee /tmp/dji-rf-research.log
\`\`\`

At the transmitter's **normal power button** (not its pairing button), switch TX1 off, on, then off while watching the existing tray "Wireless link (read-only)" status.

The optional log prints one line for each newly observed incoming USB packet *family* (frame kind, protocol marker, length, whether decoded); it also retains existing radio-link transitions. Packet payloads and serial identifiers are redacted. It does not attempt any wireless writes.

Get the relevant evidence:

\`\`\`bash
grep -E 'DJI RF trace|DJI RF read-only' /tmp/dji-rf-research.log
\`\`\`

**Limits:** Read-only RX notifications can establish the observable effects of RF link changes, but cannot by themselves reveal an outgoing RF-disable opcode. The next substantive step will require vendor firmware/control implementation analysis or USB traffic from a **known, reversible RF-disable action**. No such action has been identified in DJI Mimo yet. Do not generate command-ID guesses, enter pairing, or issue a reset to try to discover one.

Keep automatic radio sleep and any "Receiver Wireless OFF" action disabled until a command and its reversible wake path are independently verified.
