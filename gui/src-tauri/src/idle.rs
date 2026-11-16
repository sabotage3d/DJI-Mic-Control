//! Read-only PipeWire capture-activity detection for the DJI Mobile Receiver.
//!
//! The test timer never powers down or suspends the hardware. Until we have a
//! verified reversible command, an automated sleep request could leave a live
//! call without an input device or force the user to press the TX power button.
use serde_json::Value;
use std::collections::HashSet;
use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdlePhase {
    Unknown,
    Active,
    Idle,
    Due,
}

pub struct IdleMonitor {
    phase: Mutex<IdlePhase>,
}

impl IdleMonitor {
    pub fn new() -> Self {
        Self { phase: Mutex::new(IdlePhase::Unknown) }
    }

    pub fn phase(&self) -> IdlePhase {
        *self.phase.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn run_forever(&self) {
        let mut idle_since: Option<Instant> = None;
        loop {
            let next = match has_dji_audio_capture() {
                Ok(true) => {
                    idle_since = None;
                    IdlePhase::Active
                }
                Ok(false) => {
                    let since = *idle_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= Duration::from_secs(timeout_seconds()) {
                        IdlePhase::Due
                    } else {
                        IdlePhase::Idle
                    }
                }
                Err(_) => {
                    // Fail safe: device missing or PipeWire unavailable is NOT idle.
                    idle_since = None;
                    IdlePhase::Unknown
                }
            };
            let mut phase = self.phase.lock().unwrap_or_else(|p| p.into_inner());
            if *phase != next {
                match next {
                    IdlePhase::Unknown =>
                        eprintln!("[DJI idle TEST] Unknown: PipeWire unavailable or DJI source absent"),
                    IdlePhase::Active =>
                        eprintln!("[DJI idle TEST] DJI is in use; idle timer reset"),
                    IdlePhase::Idle =>
                        eprintln!("[DJI idle TEST] No capture; counting {} seconds", timeout_seconds()),
                    IdlePhase::Due =>
                        eprintln!("[DJI idle TEST] {} seconds idle: WOULD SLEEP; no USB command sent", timeout_seconds()),
                }
                *phase = next;
            }
            drop(phase);
            thread::sleep(Duration::from_secs(3));
        }
    }
}

/// 60s is the default test interval. Export DJI_IDLE_SECONDS=30 for fast tests;
/// switch to 180 when we have fully validated a reversible standby mechanism.
pub fn timeout_seconds() -> u64 {
    std::env::var("DJI_IDLE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|seconds| (10..=3600).contains(seconds))
        .unwrap_or(60)
}

fn has_dji_audio_capture() -> Result<bool, &'static str> {
    // No audio is sampled. PipeWire provides process/link metadata via JSON.
    let out = Command::new("pw-dump").arg("-N").output().map_err(|_| "pw-dump unavailable")?;
    if !out.status.success() { return Err("pw-dump failed"); }
    let graph: Vec<Value> = serde_json::from_slice(&out.stdout).map_err(|_| "bad PipeWire JSON")?;
    has_capture_link(&graph)
}

fn has_capture_link(graph: &[Value]) -> Result<bool, &'static str> {
    let dji_sources: HashSet<u64> = graph.iter().filter_map(|obj| {
        if obj.get("type")?.as_str()? != "PipeWire:Interface:Node" { return None; }
        let id = obj.get("id")?.as_u64()?;
        let props = obj.pointer("/info/props")?;
        let media = props.get("media.class").and_then(Value::as_str).unwrap_or("");
        if !media.starts_with("Audio/Source") { return None; }
        let name = props.get("node.name").and_then(Value::as_str).unwrap_or("");
        let desc = props.get("node.description").and_then(Value::as_str).unwrap_or("");
        let joined = format!("{name} {desc}").to_ascii_lowercase();
        let matches = joined.contains("wireless mic rx")
            || joined.contains("wireless_mic_rx")
            || (joined.contains("dji") && joined.contains("mic"));
        matches.then_some(id)
    }).collect();

    if dji_sources.is_empty() { return Err("DJI source not found"); }

    // Any link to the source is interpreted as use, including paused links:
    // conferencing apps can hold the mic open between speech segments.
    for obj in graph {
        if obj.get("type").and_then(Value::as_str) != Some("PipeWire:Interface:Link") { continue; }
        let output = obj.pointer("/info/output-node-id").and_then(Value::as_u64);
        if !output.is_some_and(|id| dji_sources.contains(&id)) { continue; }
        let state = obj.pointer("/info/state").and_then(Value::as_str).unwrap_or("unknown");
        if state != "error" && state != "unlinked" { return Ok(true); }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dji_capture_and_ignores_unrelated_links() {
        let source = serde_json::json!({
            "id": 95, "type": "PipeWire:Interface:Node",
            "info": {"props": {
                "media.class": "Audio/Source",
                "node.name": "alsa_input.usb-DJI_Technology_Wireless_Mic_Rx.pro-input-0"
            }}
        });
        let other = serde_json::json!({
            "id": 200, "type": "PipeWire:Interface:Link",
            "info": {"output-node-id": 45, "state": "active"}
        });
        let dji = serde_json::json!({
            "id": 201, "type": "PipeWire:Interface:Link",
            "info": {"output-node-id": 95, "state": "active"}
        });
        assert!(matches!(has_capture_link(&[source.clone(), other]), Ok(false)));
        assert!(matches!(has_capture_link(&[source.clone(), dji]), Ok(true)));
        assert!(has_capture_link(&[]).is_err());
    }
}
