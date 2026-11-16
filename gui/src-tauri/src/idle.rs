//! Read-only PipeWire capture-activity detection for the DJI Mobile Receiver.
//!
//! The idle test NEVER powers off or disconnects the transmitter. In particular,
//! a still-open Zoom stream may be a real recording stream even without a meeting.
//! We report the consumer rather than assuming that a linked device is in use.
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Probe {
    Active(Vec<String>),
    Idle,
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Reading {
    phase: IdlePhase,
    detail: String,
}

pub struct IdleMonitor {
    reading: Mutex<Reading>,
}

impl IdleMonitor {
    pub fn new() -> Self {
        Self {
            reading: Mutex::new(Reading {
                phase: IdlePhase::Unknown,
                detail: "not checked yet".into(),
            }),
        }
    }

    /// Current status and the process/stream preventing sleep (when known).
    pub fn reading(&self) -> (IdlePhase, String) {
        let reading = self.reading.lock().unwrap_or_else(|p| p.into_inner());
        (reading.phase, reading.detail.clone())
    }

    pub fn run_forever(&self) {
        let mut idle_since: Option<Instant> = None;
        loop {
            let new_reading = match probe_pipewire() {
                Ok(Probe::Active(consumers)) => {
                    idle_since = None;
                    Reading {
                        phase: IdlePhase::Active,
                        detail: consumers.join(", "),
                    }
                }
                Ok(Probe::Idle) => {
                    let since = *idle_since.get_or_insert_with(Instant::now);
                    Reading {
                        phase: if since.elapsed() >= Duration::from_secs(timeout_seconds()) {
                            IdlePhase::Due
                        } else {
                            IdlePhase::Idle
                        },
                        detail: "no application capture streams".into(),
                    }
                }
                Ok(Probe::Unknown(reason)) | Err(reason) => {
                    // Fail safe: never interpret an uncertain graph as safe to sleep.
                    idle_since = None;
                    Reading { phase: IdlePhase::Unknown, detail: reason }
                }
            };
            let mut current = self.reading.lock().unwrap_or_else(|p| p.into_inner());
            if *current != new_reading {
                match new_reading.phase {
                    IdlePhase::Active =>
                        eprintln!("[DJI idle TEST] ACTIVE: capture by {}", new_reading.detail),
                    IdlePhase::Idle =>
                        eprintln!("[DJI idle TEST] IDLE: no capture; threshold {}s", timeout_seconds()),
                    IdlePhase::Due =>
                        eprintln!("[DJI idle TEST] WOULD SLEEP after {}s; NO USB power command sent", timeout_seconds()),
                    IdlePhase::Unknown =>
                        eprintln!("[DJI idle TEST] UNKNOWN: {}", new_reading.detail),
                }
                *current = new_reading;
            }
            drop(current);
            thread::sleep(Duration::from_secs(3));
        }
    }
}

pub fn timeout_seconds() -> u64 {
    std::env::var("DJI_IDLE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|seconds| (10..=3600).contains(seconds))
        .unwrap_or(60)
}

fn probe_pipewire() -> Result<Probe, String> {
    // Process/link *metadata* only; no microphone audio samples are read.
    let output = Command::new("pw-dump")
        .arg("--no-colors")
        .output()
        .map_err(|e| format!("pw-dump unavailable: {e}"))?;
    if !output.status.success() {
        return Err(format!("pw-dump exited with {}", output.status));
    }
    let graph: Vec<Value> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid PipeWire JSON: {e}"))?;
    Ok(classify_graph(&graph))
}

fn obj_id(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse::<u64>().ok())
}

fn prop<'a>(props: &'a Value, name: &str) -> &'a str {
    props.get(name).and_then(Value::as_str).unwrap_or("")
}

fn consumer_name(node: &Value, clients: &HashMap<u64, &Value>) -> String {
    let props = node.pointer("/info/props").unwrap_or(&Value::Null);
    let client_id = props.get("client.id").and_then(obj_id);
    let client_props = client_id
        .and_then(|id| clients.get(&id).copied())
        .and_then(|client| client.pointer("/info/props"))
        .unwrap_or(&Value::Null);
    for p in [props, client_props] {
        for field in ["application.name", "application.process.binary", "media.name", "node.description", "node.name"] {
            let value = prop(p, field).trim();
            if !value.is_empty() {
                return value.chars().take(48).collect();
            }
        }
    }
    format!("PipeWire stream #{}", node.get("id").and_then(Value::as_u64).unwrap_or(0))
}

fn classify_graph(graph: &[Value]) -> Probe {
    let nodes: HashMap<u64, &Value> = graph.iter()
        .filter(|o| o.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Node"))
        .filter_map(|o| Some((o.get("id")?.as_u64()?, o)))
        .collect();
    let clients: HashMap<u64, &Value> = graph.iter()
        .filter(|o| o.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Client"))
        .filter_map(|o| Some((o.get("id")?.as_u64()?, o)))
        .collect();
    let dji_sources: HashSet<u64> = nodes.iter().filter_map(|(id, node)| {
        let props = node.pointer("/info/props")?;
        let media = prop(props, "media.class");
        if !media.starts_with("Audio/Source") { return None; }
        let name = format!("{} {}", prop(props, "node.name"), prop(props, "node.description"))
            .to_ascii_lowercase();
        (name.contains("wireless mic rx")
            || name.contains("wireless_mic_rx")
            || (name.contains("dji") && name.contains("mic"))).then_some(*id)
    }).collect();

    if dji_sources.is_empty() {
        return Probe::Unknown("DJI source not found".into());
    }

    let mut consumers = Vec::new();
    let mut uncertain = Vec::new();
    for link in graph.iter()
        .filter(|o| o.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Link"))
    {
        let Some(output) = link.pointer("/info/output-node-id").and_then(obj_id) else { continue };
        if !dji_sources.contains(&output) { continue; }
        let state = link.pointer("/info/state").and_then(Value::as_str).unwrap_or("unknown");
        if state == "error" || state == "unlinked" { continue; }
        let Some(input) = link.pointer("/info/input-node-id").and_then(obj_id) else {
            uncertain.push("link has no consumer node".to_string());
            continue;
        };
        let Some(node) = nodes.get(&input) else {
            uncertain.push(format!("linked consumer node {input} is unavailable"));
            continue;
        };
        let props = node.pointer("/info/props").unwrap_or(&Value::Null);
        let media = prop(props, "media.class");
        let category = prop(props, "media.category");

        // PipeWire Audio/Source -> Stream/Input/Audio = real app capture.
        // A muted or paused meeting may retain that stream; don't miss it.
        if media == "Stream/Input/Audio" {
            if category != "Monitor" {
                consumers.push(consumer_name(node, &clients));
            }
            continue;
        }

        // Monitoring tools and passive graph links don't themselves mean
        // that an application is recording the mic.
        let link_props = link.pointer("/info/props").unwrap_or(&Value::Null);
        let passive = matches!(prop(link_props, "link.passive"), "true" | "1");
        if category == "Monitor" || passive {
            continue;
        }

        // An echo-cancel/filter node may route into a real call indirectly.
        // Until full graph traversal is implemented, fail closed.
        uncertain.push(format!("device routed to {} ({})", consumer_name(node, &clients),
            if media.is_empty() { "unclassified" } else { media }));
    }
    consumers.sort();
    consumers.dedup();
    if !consumers.is_empty() {
        Probe::Active(consumers)
    } else if !uncertain.is_empty() {
        uncertain.sort();
        uncertain.dedup();
        Probe::Unknown(uncertain.join("; "))
    } else {
        Probe::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Value {
        serde_json::json!({
            "id": 95, "type": "PipeWire:Interface:Node",
            "info": {"props": {
                "media.class": "Audio/Source",
                "node.name": "alsa_input.usb-DJI_Technology_Wireless_Mic_Rx.pro-input-0"
            }}
        })
    }
    fn node(id: u64, class: &str, category: &str, app: &str) -> Value {
        serde_json::json!({
            "id": id, "type": "PipeWire:Interface:Node",
            "info": {"props": {
                "media.class": class, "media.category": category,
                "application.name": app
            }}
        })
    }
    fn link(id: u64, input: u64, passive: bool) -> Value {
        serde_json::json!({
            "id": id, "type": "PipeWire:Interface:Link",
            "info": {"output-node-id": 95, "input-node-id": input,
                     "state": "active", "props": {"link.passive": if passive { "true" } else { "false" }}}
        })
    }

    #[test]
    fn unlinked_source_is_idle() {
        assert_eq!(classify_graph(&[source()]), Probe::Idle);
    }
    #[test]
    fn linked_monitor_is_not_a_recording() {
        assert_eq!(classify_graph(&[source(), node(201, "Stream/Input/Audio", "Monitor", "KDE meter"), link(301, 201, false)]), Probe::Idle);
    }
    #[test]
    fn passive_internal_link_is_not_a_recording() {
        assert_eq!(classify_graph(&[source(), node(202, "Audio/Sink", "DSP", "Passive routing"), link(302, 202, true)]), Probe::Idle);
    }
    #[test]
    fn zoom_stream_is_active_even_when_muted() {
        assert_eq!(classify_graph(&[source(), node(203, "Stream/Input/Audio", "Capture", "Zoom"), link(303, 203, false)]),
            Probe::Active(vec!["Zoom".into()]));
    }
    #[test]
    fn unknown_active_filter_route_is_not_assumed_idle() {
        assert!(matches!(classify_graph(&[source(), node(204, "Audio/Sink", "DSP", "Echo canceller"), link(304, 204, false)]),
            Probe::Unknown(_)));
    }
    #[test]
    fn missing_dji_is_unknown() {
        assert!(matches!(classify_graph(&[]), Probe::Unknown(_)));
    }
}
