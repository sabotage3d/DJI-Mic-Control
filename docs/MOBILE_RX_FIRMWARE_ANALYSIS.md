# Mobile Receiver firmware and DJI Mimo binary investigation

**Target:** DJI Mic Series Mobile Receiver, USB VID:PID \`2ca3:4011\`, user-tested RX firmware **02.03.11.00**.

**Goal:** Find a *reversible* receiver RF-disable / RF-enable command that preserves TX pairing, USB audio enumeration, and automatic TX reconnect. We already verify TX1/TX2 link status from the v2 status bitmask at frame offset 44.

## Findings (2026-11-16)

- DJI publishes Mobile Receiver release notes and uses **DJI Mimo** to download firmware updates; a standalone Mobile Receiver BIN matching **02.03.11.00** was **not located** on the public product download page. Release notes are **not firmware**.
- The [official Mic Mini downloads](https://www.dji.com/downloads/products/mic-mini) provide the DJI Mimo app, manuals and release notes.
- The [official support instructions](https://www.dji.com/support/product/mic-mini) describe firmware updating via the Android receiver adapter or Bluetooth, not a generic Linux firmware-dump interface.
- DJI's [power-saving explanation](https://www.dji.com/support/product/mic-mini) confirms that TX automatically enters power saving when RX signal is lost, and that it reconnects when the signal returns. It does **not** identify a host command to make RX stop broadcasting.
- The existing protocol setting \`0x0010\` configures the **15-minute auto-off policy**, not immediate RF off. Other projects' references to HID \`0x52\` differ (reboot vs power off); neither proves reversible RF-only standby.
- Generic [DJI firmware tools](https://github.com/o-gs/dji-firmware-tools) cover several DJI drone package formats. A matching Mic Series firmware layout is **not yet confirmed**.

## Offline inspection tool

Use the dependency-free Python tool to inspect a **firmware update package or your own locally obtained DJI Mimo APK**. This never touches USB, flashes devices, or sends protocol commands.

\`\`\`bash
cd ~/Downloads/DJI-Mic-Control
git pull --ff-only
python3 tools/inspect_dji_binary.py --help
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/inspect_dji_binary.py /path/to/your/dji-mimo.apk
# Or:
python3 tools/inspect_dji_binary.py /path/to/receiver-update.bin
\`\`\`

Reports SHA-256, common file/container signatures, sample entropy, and **only keyword counts/offsets**, not raw firmware data. For ZIP/APK files it identifies which native libraries, DEX files or metadata may deserve later Ghidra/JADX inspection. It is **a triage aid, not an opcode decoder**. Firmware with stripped or encrypted symbols may show no useful plaintext.

### Obtaining an APK

Download DJI Mimo from [DJI's official page](https://www.dji.com/downloads/djiapp/dji-mimo) on a supported Android phone. If USB debugging and \`adb\` are available, identify the installed package with:

\`\`\`bash
adb shell pm path dji.mimo
\`\`\`

For each \`package:/data/app/...apk\` path it returns, \`adb pull\` may allow copying that APK to the Linux PC (access depends on Android version). If APK access is restricted, use a locally downloaded official APK file. **Do not upload proprietary APKs or firmware into the public GitHub repository.**

A normal APK contains client libraries, not necessarily the receiver firmware. The actual update is generally obtained by the app and may not be stored in an accessible directory. Do **not** initiate an update solely to gather a file; unintentionally applying a new firmware version would change our tested baseline.

## Evidence needed before hardware writes

1. A specific host-to-RX command, located in firmware/controller code or captured from a **known safe reversible RF toggle**.
2. Proof it affects only the RF link, not pairing memory or device power.
3. A verified **wake path** from Linux with RX USB still connected.
4. A controlled test showing the existing TX status transitions from connected to no-TX and back, and that audio resumes.
5. Fail-safe behavior on app exit, restart, crashing, unknown PipeWire activity or an active meeting.

Until then there must be **no RF-off GUI button, guessed command, HID reboot, or hidden firmware write**.
