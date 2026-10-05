//! Messages between the Talkr app and its engine process (`talkr-engine`).
//!
//! The speech engines (whisper.cpp, sherpa-onnx) are native code. Running them in a child
//! process means a crash, an out-of-memory kill or a broken GPU driver ends that process, not
//! the app: the app reports it and starts a fresh engine. The two sides talk over the engine's
//! stdin/stdout, one JSON object per line. The engine's stderr carries its log.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// A request from the app. `id` ties the replies to it; for `Cancel` it names the job to stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub id: String,
    #[serde(flatten)]
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    /// Report the compute devices the engine can use. Answered with `Event::Devices`.
    Probe,
    Transcribe(TranscribeJob),
    Synthesize(SynthesizeJob),
    /// List a TTS model's voices. Answered with `Event::Voices`.
    Voices(ModelRef),
    /// Stop the running job whose id is this request's `id`. No reply of its own: the job ends
    /// with `Event::Failed { kind: Cancelled }`.
    Cancel,
    /// Free the loaded models. Answered with `Event::Unloaded`.
    Unload,
}

/// Which model to use and how to run it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub model_id: String,
    pub dir: PathBuf,
    pub threads: usize,
    /// Run on the GPU when the engine has one (speech to text only; TTS always uses the CPU).
    pub gpu: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscribeJob {
    pub model: ModelRef,
    pub audio_path: PathBuf,
    /// ISO 639-1 code, or `None`/"auto" to detect.
    pub language: Option<String>,
    pub translate: bool,
    #[serde(default)]
    pub decoding: Decoding,
}

/// How hard whisper searches for the transcript.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decoding {
    /// The single most likely word at each step: fastest.
    #[default]
    Greedy,
    /// Keep several candidate transcripts and pick the best: more accurate, about 1.5x slower on
    /// the CPU (see the engine's `bench_decoding_modes` test).
    Beam,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeJob {
    pub model: ModelRef,
    pub text: String,
    pub voice_id: String,
    pub speed: f32,
    /// Where the engine writes the resulting WAV.
    pub out_path: PathBuf,
}

/// A message from the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum Event {
    /// Sent once at start-up, before any request is read.
    Ready { version: String },
    Progress { id: String, progress: f32 },
    Devices { id: String, devices: Vec<Device> },
    Transcribed { id: String, transcript: Transcript, audio_ms: i64, device: String },
    Synthesized { id: String, sample_rate: u32, duration_ms: i64, device: String },
    Voices { id: String, voices: Vec<Voice> },
    Unloaded { id: String },
    Failed { id: String, kind: FailureKind, error: String },
}

impl Event {
    /// The request this event answers (`None` for `Ready`).
    pub fn id(&self) -> Option<&str> {
        match self {
            Event::Ready { .. } => None,
            Event::Progress { id, .. }
            | Event::Devices { id, .. }
            | Event::Transcribed { id, .. }
            | Event::Synthesized { id, .. }
            | Event::Voices { id, .. }
            | Event::Unloaded { id }
            | Event::Failed { id, .. } => Some(id),
        }
    }

    /// Whether this is the last event for its request.
    pub fn is_final(&self) -> bool {
        !matches!(self, Event::Ready { .. } | Event::Progress { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureKind {
    Cancelled,
    /// Bad input: unreadable audio, empty text, a model folder that is not a model.
    Invalid,
    /// The engine could not allocate what the model needs.
    OutOfMemory,
    Engine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Cpu,
    /// A discrete GPU with its own memory.
    Gpu,
    /// An integrated GPU sharing system memory.
    Igpu,
    Accelerator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub kind: DeviceKind,
    /// Backend name, e.g. "Vulkan0" or "Metal".
    pub name: String,
    pub description: String,
    pub memory_free: u64,
    pub memory_total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: Option<String>,
}

/// Encode a message as one line of JSON (with the trailing newline).
pub fn to_line<T: Serialize>(msg: &T) -> String {
    // Serializing these plain data types cannot fail.
    let mut line = serde_json::to_string(msg).expect("protocol messages always serialize");
    line.push('\n');
    line
}

/// Decode one line of JSON.
pub fn from_line<'a, T: Deserialize<'a>>(line: &'a str) -> serde_json::Result<T> {
    serde_json::from_str(line.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ModelRef {
        ModelRef { model_id: "whisper-tiny-en".into(), dir: "/m/tiny".into(), threads: 4, gpu: true }
    }

    #[test]
    fn requests_round_trip() {
        let requests = [
            Request { id: "1".into(), op: Op::Probe },
            Request {
                id: "2".into(),
                op: Op::Transcribe(TranscribeJob {
                    model: model(),
                    audio_path: "/a.wav".into(),
                    language: Some("en".into()),
                    translate: false,
                    decoding: Decoding::Beam,
                }),
            },
            Request {
                id: "3".into(),
                op: Op::Synthesize(SynthesizeJob {
                    model: model(),
                    text: "Hi.\nThere \"quoted\" ünïcode".into(),
                    voice_id: "af_bella".into(),
                    speed: 1.25,
                    out_path: "/o.wav".into(),
                }),
            },
            Request { id: "4".into(), op: Op::Voices(model()) },
            Request { id: "2".into(), op: Op::Cancel },
            Request { id: "5".into(), op: Op::Unload },
        ];
        for request in requests {
            let line = to_line(&request);
            assert!(line.ends_with('\n') && !line.trim_end().contains('\n'), "one line: {line}");
            assert_eq!(from_line::<Request>(&line).unwrap(), request);
        }
    }

    #[test]
    fn wire_format_is_stable() {
        // The app and engine ship together, but keep the shape explicit.
        let line = to_line(&Request { id: "7".into(), op: Op::Cancel });
        assert_eq!(line, "{\"id\":\"7\",\"op\":\"cancel\"}\n");
        let event: Event = from_line(r#"{"event":"progress","id":"7","progress":0.5}"#).unwrap();
        assert_eq!(event, Event::Progress { id: "7".into(), progress: 0.5 });
    }

    #[test]
    fn decoding_defaults_to_greedy() {
        let line = r#"{"id":"1","op":"transcribe","model":{"modelId":"m","dir":"d","threads":1,"gpu":false},"audioPath":"a.wav","language":null,"translate":false}"#;
        match from_line::<Request>(line).unwrap().op {
            Op::Transcribe(job) => assert_eq!(job.decoding, Decoding::Greedy),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn events_round_trip_and_classify() {
        let events = [
            Event::Ready { version: "0.1.2".into() },
            Event::Progress { id: "1".into(), progress: 0.25 },
            Event::Devices {
                id: "1".into(),
                devices: vec![Device {
                    kind: DeviceKind::Gpu,
                    name: "Vulkan0".into(),
                    description: "NVIDIA GeForce RTX 3060".into(),
                    memory_free: 11 << 30,
                    memory_total: 12 << 30,
                }],
            },
            Event::Transcribed {
                id: "1".into(),
                transcript: Transcript {
                    text: "hello".into(),
                    segments: vec![TranscriptSegment { start_ms: 0, end_ms: 900, text: "hello".into() }],
                    language: Some("en".into()),
                },
                audio_ms: 900,
                device: "cpu".into(),
            },
            Event::Synthesized { id: "1".into(), sample_rate: 22050, duration_ms: 1200, device: "cpu".into() },
            Event::Voices {
                id: "1".into(),
                voices: vec![Voice { id: "0".into(), name: "Amy".into(), language: "en-US".into(), gender: None }],
            },
            Event::Unloaded { id: "1".into() },
            Event::Failed { id: "1".into(), kind: FailureKind::OutOfMemory, error: "no memory".into() },
        ];
        for event in events {
            let back: Event = from_line(&to_line(&event)).unwrap();
            assert_eq!(back, event);
            let is_ready_or_progress = matches!(event, Event::Ready { .. } | Event::Progress { .. });
            assert_eq!(event.is_final(), !is_ready_or_progress);
            assert_eq!(event.id().is_none(), matches!(event, Event::Ready { .. }));
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(from_line::<Event>("not json").is_err());
        assert!(from_line::<Request>(r#"{"id":"1","op":"explode"}"#).is_err());
    }
}
