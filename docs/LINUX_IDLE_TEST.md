# Linux: Stage 2 gain + PipeWire idle test

This branch adds a five-step snapping **hardware gain slider** (−12, −6, 0, +6, +12 dB), a **Receiver Gain** submenu in the native tray, and a read-only PipeWire capture-activity monitor.

**The idle monitor does NOT power off, disconnect or sleep the receiver/transmitter.** No verified reversible sleep/wake command has been established. The status only reports **WOULD SLEEP**.

## Build on Arch

```bash
cd ~/Downloads/DJI-Mic-Control
git pull --ff-only origin feature/linux-gain-tray-idle
(cd gui && npm run build)
cargo test -p djimic-gui
cargo build --release -p djimic-gui --features custom-protocol
```

## Test 30 seconds first

```bash
pkill -x djimic-gui 2>/dev/null || true
DJI_IDLE_SECONDS=30 ./target/release/djimic-gui 2>&1 | tee /tmp/dji-mic-test.log
```

Default is **60 seconds**; for a 3-minute test, set `DJI_IDLE_SECONDS=180`. The override applies when starting the process and is not configured by the GUI.

The tray's right-click menu displays the gain steps and current idle status. The main GUI has the snapping gain slider. `Start at login (hidden)` is an existing option; the Linux NVIDIA/WebKit workaround is now applied in the app itself.

The monitor uses `pw-dump -N` to identify the DJI microphone source and its active PipeWire links. If PipeWire is unavailable or the device cannot be identified, status is **Unknown**, not idle. Recordings in Zoom/OBS should show **recording active** while they retain a PipeWire link. Verify on your installation before attempting actual power management.

When the idle monitor appears incorrect, collect `wpctl status`, a redacted `pw-dump -N`, and `/tmp/dji-mic-test.log`. Redact device serials or unrelated device metadata before sharing.


## Activity detection troubleshooting (Stage 2.1)

The previous implementation interpreted **any** PipeWire link from the DJI source as "recording active". Stage 2.1 distinguishes application capture streams (\`media.class=Stream/Input/Audio\`) from monitoring/passive/internal links. The KDE tray shows the name of the consumer keeping the microphone active, for example \`Idle test: in use by Zoom\`.

**Keep in mind:** Zoom may leave an actual capture stream open while its app is running, even when not in a meeting. A muted stream is also treated as in use to avoid shutting down an ongoing call. If this happens, the tray and terminal identify Zoom so we can investigate it specifically rather than ignoring all Zoom streams.

Unknown active filter routes are shown as **Unknown**, not **Idle**, because blindly disregarding a DSP/echo-cancellation link could interrupt real recordings.

The test timer remains read-only. There is **no verified power-off / reversible wake USB command**, and no sleep command will be sent by this version.
