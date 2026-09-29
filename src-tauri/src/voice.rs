use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub provider: String,
    pub model_path: String,
    pub endpoint: String,
    pub model: String,
    pub cloud_audio: bool,
    pub microphone: Option<u32>,
    pub voice: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: "local".into(),
            model_path: default_model(),
            endpoint: String::new(),
            model: String::new(),
            cloud_audio: false,
            microphone: None,
            voice: String::new(),
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub recognition: Settings,
    pub synthesis: Settings,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        for c in [&self.recognition, &self.synthesis] {
            if !["local", "cloud"].contains(&c.provider.as_str()) {
                return Err("Unknown speech provider".into());
            }
            if c.provider == "cloud" {
                let url =
                    reqwest::Url::parse(&c.endpoint).map_err(|_| "Invalid speech endpoint")?;
                if !c.cloud_audio || url.scheme() != "https" {
                    return Err("Cloud audio needs explicit consent and HTTPS".into());
                }
            }
        }
        Ok(())
    }
}
pub fn python() -> String {
    if let Ok(value) = std::env::var("CLICKY_PYTHON") {
        return value;
    }
    if cfg!(debug_assertions) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
            "../.venv/Scripts/python.exe"
        } else {
            "../.venv/bin/python"
        });
        if path.exists() {
            return path.to_string_lossy().into_owned();
        }
    }
    if cfg!(windows) {
        "python".into()
    } else {
        "python3".into()
    }
}
fn default_model() -> String {
    std::env::var("CLICKY_STT_MODEL").unwrap_or_else(|_| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../.models/vosk-model-small-en-us-0.15");
        if cfg!(debug_assertions) && path.is_dir() {
            path.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    })
}
pub fn script(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    if cfg!(debug_assertions) {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../voice/worker.py"))
    } else {
        Ok(app
            .path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("voice/worker.py"))
    }
}
/// Dropping this future terminates the process, closing capture/output devices.
pub async fn run(path: PathBuf, request: Value, on_state: impl Fn(&str)) -> Result<Value, String> {
    let operation = request["operation"].as_str().unwrap_or("");
    let timeout = if operation == "listen" {
        50
    } else if operation == "probe" {
        8
    } else {
        30
    };
    let python = python();
    let mut command = tokio::process::Command::new(python);
    command
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| "Voice needs Python. Set CLICKY_PYTHON and install voice/requirements.txt.")?;
    let work = async {
        let mut bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        let mut stdin = child.stdin.take().ok_or("Voice input unavailable")?;
        stdin.write_all(&bytes).await.map_err(|e| e.to_string())?;
        drop(stdin);
        let mut reader = BufReader::new(child.stdout.take().ok_or("Voice output unavailable")?);
        let mut result = None;
        for _ in 0..16 {
            // Bound each line before allocating arbitrary worker output.
            let mut line = Vec::new();
            loop {
                let available = reader.fill_buf().await.map_err(|e| e.to_string())?;
                if available.is_empty() {
                    break;
                }
                let count = available
                    .iter()
                    .position(|v| *v == b'\n')
                    .map(|n| n + 1)
                    .unwrap_or(available.len());
                if line.len() + count > 65536 {
                    return Err("Voice output exceeded limit".into());
                }
                line.extend_from_slice(&available[..count]);
                let end = line.last() == Some(&b'\n');
                reader.consume(count);
                if end {
                    break;
                }
            }
            if line.is_empty() {
                break;
            }
            let event: Value =
                serde_json::from_slice(&line).map_err(|_| "Invalid voice protocol")?;
            match event["event"].as_str() {
                Some("state") => on_state(event["state"].as_str().unwrap_or("")),
                Some("error") => {
                    return Err(event["message"]
                        .as_str()
                        .unwrap_or("Voice service failed")
                        .chars()
                        .take(500)
                        .collect())
                }
                Some("result") => {
                    result = Some(event);
                    break;
                }
                _ => return Err("Unknown voice event".into()),
            }
        }
        if !child.wait().await.map_err(|e| e.to_string())?.success() {
            return Err("Voice process exited unexpectedly".into());
        }
        result.ok_or("Voice process ended without a result".into())
    };
    tokio::time::timeout(Duration::from_secs(timeout), work)
        .await
        .map_err(|_| "Voice timed out; microphone and speech stopped".to_string())?
}
pub async fn speak(app: &tauri::AppHandle, text: &str, config: Settings) -> Result<(), String> {
    run(
        script(app)?,
        json!({"operation":"speak","text":text,"config":config}),
        |_| {},
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    #[test]
    fn cancellation_terminates_worker_not_only_its_ui() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let dir=std::env::temp_dir().join(format!("clicky-voice-{stamp}"));std::fs::create_dir(&dir).unwrap();
            let script=dir.join("worker.py");let heartbeat=dir.join("heartbeat");
            std::fs::write(&script,"import sys,json,time\nfrom pathlib import Path\njson.loads(sys.stdin.readline())\np=Path(__file__).parent/'heartbeat'\np.write_text('0')\nprint('{\"event\":\"state\",\"state\":\"LISTENING\"}',flush=True)\nfor n in range(1000):\n p.write_text(str(n))\n time.sleep(.05)\n").unwrap();
            let ready=Arc::new(AtomicBool::new(false));let signal=ready.clone();
            let job=tokio::spawn(async move{run(script,json!({"operation":"listen"}),move |_|{signal.store(true,Ordering::SeqCst);}).await});
            tokio::time::timeout(Duration::from_secs(5),async{while !ready.load(Ordering::SeqCst){tokio::time::sleep(Duration::from_millis(10)).await;}}).await.unwrap();
            job.abort();let _=job.await;
            tokio::time::sleep(Duration::from_millis(200)).await;let stopped=std::fs::read_to_string(&heartbeat).unwrap();
            tokio::time::sleep(Duration::from_millis(250)).await;assert_eq!(stopped,std::fs::read_to_string(&heartbeat).unwrap());
            std::fs::remove_file(dir.join("worker.py")).unwrap();std::fs::remove_file(heartbeat).unwrap();std::fs::remove_dir(dir).unwrap();
        });
    }
    #[test]
    fn voice_cloud_requires_its_own_consent() {
        let mut config = Config::default();
        config.recognition.provider = "cloud".into();
        config.recognition.endpoint = "https://example.invalid/transcribe".into();
        assert!(config.validate().is_err());
        config.recognition.cloud_audio = true;
        assert!(config.validate().is_ok());
    }
}
