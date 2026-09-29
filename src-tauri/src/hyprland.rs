//! Hyprland coordinates are compositor logical pixels. grim -s 1 preserves that
//! space through OCR and layer-shell rendering, including mixed-scale outputs.
use crate::{
    task::{Bounds, Step},
    Runtime,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub fn available() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}
async fn output(program: &str, args: &[&str], limit: u64) -> Result<Vec<u8>, String> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| format!("Install {program} for Hyprland perception"))?;
    let result = tokio::time::timeout(Duration::from_secs(8), async {
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or("Missing command output")?
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit {
            return Err("Desktop response exceeded size limit".into());
        }
        if !child.wait().await.map_err(|e| e.to_string())?.success() {
            return Err(format!(
                "{program} failed; check Hyprland session and capture permissions"
            ));
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| format!("{program} timed out"))?;
    result
}
async fn query(name: &str) -> Result<Value, String> {
    serde_json::from_slice(&output("hyprctl", &["-j", name], 1024 * 1024).await?)
        .map_err(|_| "Invalid Hyprland response".into())
}
pub fn region(window: &Value) -> Result<Bounds, String> {
    let number = |field: &str, index: usize| {
        window[field][index]
            .as_f64()
            .ok_or("Active application has no geometry".to_string())
    };
    let bounds = Bounds {
        x: number("at", 0)?,
        y: number("at", 1)?,
        width: number("size", 0)?,
        height: number("size", 1)?,
    };
    if !bounds.valid() || bounds.width * bounds.height > 24_000_000. {
        return Err("Invalid active-window capture size".into());
    }
    if window["class"]
        .as_str()
        .unwrap_or("")
        .contains("com.samash")
        || window["title"]
            .as_str()
            .unwrap_or("")
            .starts_with("ClickyAI")
    {
        return Err("Focus the application you want help with before activating ClickyAI".into());
    }
    Ok(bounds)
}
pub async fn capture() -> Result<(Vec<u8>, (f64, f64)), String> {
    let window = query("activewindow").await?;
    let r = region(&window)?;
    let geometry = format!(
        "{},{} {}x{}",
        r.x as i64, r.y as i64, r.width as u64, r.height as u64
    );
    let bytes = output(
        "grim",
        &["-s", "1", "-g", &geometry, "-t", "png", "-"],
        8 * 1024 * 1024,
    )
    .await?;
    Ok((bytes, (r.x, r.y)))
}
async fn desktop_python() -> Result<String, String> {
    let mut candidates = Vec::new();
    if let Ok(p) = std::env::var("CLICKY_DESKTOP_PYTHON") {
        candidates.push(PathBuf::from(p));
    } else if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(
            std::env::split_paths(&path)
                .map(|p| p.join("python3"))
                .filter(|p| p.is_file()),
        );
    }
    for p in candidates {
        let name = p.to_string_lossy().into_owned();
        if output(
            &name,
            &[
                "-c",
                "import gi, cairo; gi.require_version('GtkLayerShell','0.1')",
            ],
            1024,
        )
        .await
        .is_ok()
        {
            return Ok(name);
        }
    }
    Err("Native overlay needs python-gobject, python-cairo and gtk-layer-shell. Set CLICKY_DESKTOP_PYTHON if needed.".into())
}
pub async fn show(app: &AppHandle, step: Step, generation: u64) -> Result<(), String> {
    let bounds = step.target.as_ref().ok_or("No selected target")?.bounds;
    let monitors = query("monitors").await?;
    let monitor = monitors
        .as_array()
        .ok_or("No monitors")?
        .iter()
        .find(|m| {
            let scale = m["scale"].as_f64().unwrap_or(1.);
            let rotated = m["transform"].as_i64().unwrap_or(0) % 2 != 0;
            let (w, h) = if rotated {
                ("height", "width")
            } else {
                ("width", "height")
            };
            Bounds {
                x: m["x"].as_f64().unwrap_or(0.),
                y: m["y"].as_f64().unwrap_or(0.),
                width: m[w].as_f64().unwrap_or(0.) / scale,
                height: m[h].as_f64().unwrap_or(0.) / scale,
            }
            .contains(bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.)
        })
        .ok_or("Target monitor unavailable")?;
    let script = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../desktop/hyprland_overlay.py")
    } else {
        app.path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("desktop/hyprland_overlay.py")
    };
    let mut child = tokio::process::Command::new(desktop_python().await?)
        .arg(script)
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Cannot start native layer-shell overlay")?;
    let mut input = child.stdin.take().ok_or("Overlay input unavailable")?;
    input
        .write_all(
            format!(
                "{}\n",
                json!({"bounds":bounds,"monitor":monitor,"instruction":step.instruction})
            )
            .as_bytes(),
        )
        .await
        .map_err(|e| e.to_string())?;
    drop(input);
    let mut reader =
        BufReader::new(child.stdout.take().ok_or("Overlay output unavailable")?).lines();
    let first = tokio::time::timeout(Duration::from_secs(5), reader.next_line())
        .await
        .map_err(|_| "Overlay startup timed out")?
        .map_err(|e| e.to_string())?;
    if !first
        .as_deref()
        .and_then(|line| serde_json::from_str::<Value>(line).ok())
        .is_some_and(|value| value["event"] == "ready")
    {
        return Err(
            "Native overlay could not initialize; check GTK layer-shell dependencies".into(),
        );
    }
    let handle = app.clone();
    let job = tauri::async_runtime::spawn(async move {
        let _child = child;
        while let Ok(Some(line)) = reader.next_line().await {
            if handle
                .state::<Runtime>()
                .generation
                .load(std::sync::atomic::Ordering::SeqCst)
                != generation
            {
                return;
            }
            match serde_json::from_str::<Value>(&line)
                .ok()
                .and_then(|v| v["event"].as_str().map(str::to_owned))
                .as_deref()
            {
                Some("confirmed") => {
                    crate::confirm_hyprland_step(&handle, step.id);
                    return;
                }
                Some("cancel") => {
                    crate::cancel_task(handle.clone());
                    return;
                }
                _ => break,
            }
        }
        crate::fail(
            &handle,
            "Native overlay disconnected. Activate ClickyAI to try again.".into(),
        );
    });
    *app.state::<Runtime>().overlay_job.lock().unwrap() = Some(job);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_uses_logical_negative_origin() {
        let r = region(&json!({"at":[-1280,40],"size":[900,700],"class":"editor"})).unwrap();
        assert_eq!(r.x, -1280.);
        assert_eq!(r.width, 900.);
    }
    #[test]
    fn reject_own_or_invalid_window() {
        assert!(region(&json!({"at":[0,0],"size":[1,1],"title":"ClickyAI Settings"})).is_err());
        assert!(region(&json!({"at":[0,0],"size":[0,1]})).is_err());
    }
}

/// Small native status layer. Replacing it cancels its owned process; never takes
/// the application's focus like a normal xdg_toplevel status window can.
pub fn status(app: &AppHandle, message: Option<String>) {
    let state = app.state::<Runtime>();
    if let Some(job) = state.status_job.lock().unwrap().take() {
        job.abort();
    }
    let Some(message) = message else {
        return;
    };
    let handle = app.clone();
    let job = tauri::async_runtime::spawn(async move {
        let result = async {
            let python = desktop_python().await?;
            let script = if cfg!(debug_assertions) {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../desktop/hyprland_status.py")
            } else {
                handle
                    .path()
                    .resource_dir()
                    .map_err(|e| e.to_string())?
                    .join("desktop/hyprland_status.py")
            };
            let mut child = tokio::process::Command::new(python)
                .arg(script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| e.to_string())?;
            let mut input = child.stdin.take().ok_or("Status input unavailable")?;
            input
                .write_all(format!("{}\n", json!({"message":message})).as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            drop(input);
            let mut lines =
                BufReader::new(child.stdout.take().ok_or("Status output unavailable")?).lines();
            if let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
                if serde_json::from_str::<Value>(&line)
                    .ok()
                    .is_some_and(|v| v["event"] == "cancel")
                {
                    crate::cancel_task(handle.clone());
                }
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = result {
            *handle.state::<Runtime>().voice_error.lock().unwrap() = Some(error);
        }
    });
    *state.status_job.lock().unwrap() = Some(job);
}
