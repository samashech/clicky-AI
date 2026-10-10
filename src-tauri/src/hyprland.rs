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
    output_for(program, args, limit, 8).await
}
async fn output_for(
    program: &str,
    args: &[&str],
    limit: u64,
    seconds: u64,
) -> Result<Vec<u8>, String> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| format!("Install {program} for Hyprland perception"))?;
    let result = tokio::time::timeout(Duration::from_secs(seconds), async {
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
pub fn same_window(expected: &Value, current: &Value) -> bool {
    expected["address"].as_str().is_some_and(|s| !s.is_empty())
        && expected["pid"].as_u64().is_some_and(|p| p > 0)
        && current["mapped"].as_bool() == Some(true)
        && current["hidden"].as_bool() != Some(true)
        && ["address", "pid", "at", "size", "monitor"]
            .iter()
            .all(|key| expected[*key] == current[*key])
}
pub async fn capture() -> Result<(Vec<u8>, (f64, f64), Value), String> {
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
    if !same_window(&window, &query("activewindow").await?) {
        return Err("The application changed during capture. Focus it and try again.".into());
    }
    Ok((bytes, (r.x, r.y), window))
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
    let window = app
        .state::<Runtime>()
        .capture_window
        .lock()
        .unwrap()
        .clone()
        .ok_or("Capture context unavailable")?;
    if !same_window(&window, &query("activewindow").await?) {
        return Err(
            "The application moved or changed while I was looking. Focus it and try again.".into(),
        );
    }
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
                json!({"bounds":bounds,"monitor":monitor,"instruction":step.instruction,"window":window,"destination":step.destination,"action":step.expected_action,"expected_result":step.expected_result,"source":step.target.as_ref().map(|t|t.source.clone())})
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
                Some("recheck") => {
                    crate::recheck_hyprland_step(&handle, step.id);
                    return;
                }
                Some("stale") => {
                    crate::invalidate_hyprland_step(&handle, step.id);
                }
                Some("cancel") => {
                    crate::cancel_task(handle.clone());
                    return;
                }
                _ => break,
            }
        }
        if handle
            .state::<Runtime>()
            .generation
            .load(std::sync::atomic::Ordering::SeqCst)
            != generation
        {
            return;
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
    fn capture_context_rejects_focus_or_geometry_changes() {
        let original = json!({"address":"0x123","pid":42,"at":[-900,0],"size":[900,700],"monitor":1,"mapped":true});
        assert!(same_window(&original, &original));
        for (key, value) in [
            ("address", json!("0x456")),
            ("pid", json!(43)),
            ("at", json!([0, 0])),
            ("size", json!([900, 701])),
            ("mapped", json!(false)),
            ("hidden", json!(true)),
        ] {
            let mut changed = original.clone();
            changed[key] = value;
            assert!(!same_window(&original, &changed), "{key}");
        }
        assert!(!same_window(&json!({}), &json!({})));
    }
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

/// User-drawn region only; selecting outside the focused application is rejected.
pub async fn capture_selected() -> Result<(Vec<u8>, (f64, f64), Value), String> {
    let window = query("activewindow").await?;
    let active = region(&window)?;
    let selected=output_for("slurp",&["-f","%x,%y %wx%h"],256,45).await.map_err(|_|"Region selection cancelled or unavailable. Install slurp, then select within the active application.".to_string())?;
    let text = std::str::from_utf8(&selected)
        .map_err(|_| "Invalid selection")?
        .trim();
    let parts: Vec<_> = text
        .split([',', ' ', 'x'])
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() != 4 {
        return Err("Invalid selection geometry".into());
    }
    let v: Vec<f64> = parts
        .iter()
        .map(|s| s.parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| "Invalid selection geometry")?;
    let r = Bounds {
        x: v[0],
        y: v[1],
        width: v[2],
        height: v[3],
    };
    if !r.valid()
        || r.width < 8.
        || r.height < 8.
        || r.x < active.x
        || r.y < active.y
        || r.x + r.width > active.x + active.width
        || r.y + r.height > active.y + active.height
    {
        return Err("Select a region inside the active application's window.".into());
    }
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
    if !same_window(&window, &query("activewindow").await?) {
        return Err("The selected application changed. Please select again.".into());
    }
    Ok((bytes, (r.x, r.y), window))
}
