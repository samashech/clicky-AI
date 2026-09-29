use crate::{
    session::{Phase, Session},
    task::Task,
    voice, Runtime,
};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

pub fn phase(app: &AppHandle, next: Phase, message: impl Into<String>) {
    let state = app.state::<Runtime>();
    let snapshot = {
        let mut session = state.session.lock().unwrap();
        if !session.transition(next, message) {
            return;
        }
        session.clone()
    };
    let _ = app.emit("runtime-state", &snapshot);
    if let Some(tray) = app.tray_by_id("clicky") {
        let _ = tray.set_tooltip(Some(format!("ClickyAI · {:?}", next)));
    }
    if let Some(window) = app.get_webview_window("indicator") {
        if matches!(next, Phase::Idle | Phase::Paused) {
            let _ = window.hide();
        } else {
            if !crate::platform::wayland() {
                if let Some((x, y)) = *state.pointer.lock().unwrap() {
                    if let Ok(Some(monitor)) = window.monitor_from_point(x, y) {
                        let origin = monitor.position();
                        let size = monitor.size();
                        let scale = monitor.scale_factor();
                        let px = (x + 24.).clamp(
                            origin.x as f64,
                            (origin.x + size.width as i32) as f64 - 340. * scale,
                        );
                        let py = (y + 28.).clamp(
                            origin.y as f64,
                            (origin.y + size.height as i32) as f64 - 96. * scale,
                        );
                        let _ =
                            window.set_position(tauri::PhysicalPosition::new(px as i32, py as i32));
                    }
                }
            }
            let _ = window.show();
            let _ = window.set_focusable(false);
        }
    }
}
fn escape(app: &AppHandle, enable: bool) {
    if crate::platform::wayland() {
        return;
    }
    if enable {
        let _ = app.global_shortcut().register("Escape");
    } else {
        let _ = app.global_shortcut().unregister("Escape");
    }
}
pub fn idle(app: &AppHandle) {
    escape(app, false);
    phase(app, Phase::Idle, "");
    crate::hide_overlay(app);
}
#[tauri::command]
pub fn runtime_state(app: AppHandle) -> Session {
    app.state::<Runtime>().session.lock().unwrap().clone()
}
#[tauri::command]
pub fn activate(app: AppHandle) {
    let state = app.state::<Runtime>();
    let snapshot = state.session.lock().unwrap().clone();
    if snapshot.phase == Phase::Paused {
        return;
    }
    if snapshot.active() {
        crate::cancel_task(app);
        return;
    }
    crate::stop(&app);
    escape(&app, true);
    phase(&app, Phase::Activating, "Starting microphone…");
    if let Some(window) = app.get_webview_window("control") {
        let _ = window.hide();
    }
    let generation = state.generation.load(Ordering::SeqCst);
    let config = state.voice_config.lock().unwrap().recognition.clone();
    let handle = app.clone();
    let job = tauri::async_runtime::spawn(async move {
        let result = async {
            let response = voice::run(
                voice::script(&handle)?,
                serde_json::json!({"operation":"listen","config":config}),
                |stage| {
                    if handle.state::<Runtime>().generation.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    match stage {
                        "LISTENING" => phase(&handle, Phase::Listening, "Listening…"),
                        "TRANSCRIBING" => {
                            phase(&handle, Phase::Transcribing, "Understanding your words…")
                        }
                        _ => {}
                    }
                },
            )
            .await?;
            let text = response["text"]
                .as_str()
                .ok_or("No transcription returned")?;
            phase(&handle, Phase::Thinking, "Thinking…");
            begin(&handle, text, "guide")?;
            crate::ground(handle.clone()).await;
            Ok::<(), String>(())
        }
        .await;
        if handle.state::<Runtime>().generation.load(Ordering::SeqCst) == generation {
            if let Err(error) = result {
                crate::fail(&handle, error);
            }
        }
    });
    *state.job.lock().unwrap() = Some(job);
}
pub fn begin(app: &AppHandle, goal: &str, mode: &str) -> Result<(), String> {
    let state = app.state::<Runtime>();
    if crate::platform::wayland() {
        return Err("I heard you, but native Wayland screen guidance is not available yet. Voice and tray controls remain available.".into());
    }
    if state.input_error.lock().unwrap().is_some() {
        return Err("Global pointer observation is unavailable. Check Diagnostics.".into());
    }
    escape(app, true);
    let goal = crate::task::normalize_voice_goal(goal);
    let task = Task::new(state.generation.load(Ordering::SeqCst), &goal, mode)?;
    *state.task.lock().unwrap() = Some(task);
    state.model_calls.store(0, Ordering::SeqCst);
    Ok(())
}
#[tauri::command]
pub fn pause(app: AppHandle) {
    let was_paused = app.state::<Runtime>().session.lock().unwrap().phase == Phase::Paused;
    crate::cancel_task(app.clone());
    phase(
        &app,
        if was_paused {
            Phase::Idle
        } else {
            Phase::Paused
        },
        "",
    );
}
#[tauri::command]
pub fn open_settings(app: AppHandle, section: Option<String>) {
    crate::cancel_task(app.clone());
    crate::control(&app);
    let _ = app.emit_to("control", "settings-section", section.unwrap_or_default());
}
#[tauri::command]
pub fn configure_voice(app: AppHandle, config: voice::Config) -> Result<(), String> {
    config.validate()?;
    crate::cancel_task(app.clone());
    *app.state::<Runtime>().voice_config.lock().unwrap() = config;
    Ok(())
}
#[tauri::command]
pub fn voice_settings(app: AppHandle) -> voice::Config {
    app.state::<Runtime>().voice_config.lock().unwrap().clone()
}
#[tauri::command]
pub async fn voice_diagnostics(app: AppHandle) -> Result<serde_json::Value, String> {
    voice::run(
        voice::script(&app)?,
        serde_json::json!({"operation":"probe"}),
        |_| {},
    )
    .await
    .map(|v| v["diagnostics"].clone())
}
pub fn finish(app: &AppHandle) {
    crate::hide_overlay(app);
    phase(app, Phase::Completed, "Done");
    let handle = app.clone();
    let generation = app.state::<Runtime>().generation.load(Ordering::SeqCst);
    let config = app
        .state::<Runtime>()
        .voice_config
        .lock()
        .unwrap()
        .synthesis
        .clone();
    let job = tauri::async_runtime::spawn(async move {
        phase(&handle, Phase::Speaking, "Done");
        let result = voice::speak(&handle, "Done.", config).await;
        if let Err(error) = result {
            *handle.state::<Runtime>().voice_error.lock().unwrap() = Some(error);
        }
        if handle.state::<Runtime>().generation.load(Ordering::SeqCst) == generation {
            phase(&handle, Phase::Completed, "Done");
            tokio::time::sleep(std::time::Duration::from_millis(900)).await;
            idle(&handle);
        }
    });
    *app.state::<Runtime>().job.lock().unwrap() = Some(job);
}
pub fn error(app: &AppHandle, message: String) {
    escape(app, false);
    phase(app, Phase::Error, message);
    let handle = app.clone();
    let generation = app.state::<Runtime>().generation.load(Ordering::SeqCst);
    // Independent short timer is epoch guarded; it never touches a newer session.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        if handle.state::<Runtime>().generation.load(Ordering::SeqCst) == generation {
            idle(&handle);
        }
    });
}
pub fn tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::TrayIconBuilder,
    };
    let activate = MenuItem::with_id(app, "activate", "Activate Clicky", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause / Resume", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let diagnostics = MenuItem::with_id(app, "diagnostics", "Diagnostics", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&activate, &pause, &settings, &diagnostics, &quit])?;
    let mut pixels = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let dx = x as f64 - 15.5;
            let dy = y as f64 - 15.5;
            if dx * dx + dy * dy < 180. {
                let i = (y * 32 + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[172, 201, 255, 255]);
                if (10..=13).contains(&x) && (8..=11).contains(&y) {
                    pixels[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
    }
    TrayIconBuilder::with_id("clicky")
        .icon(tauri::image::Image::new_owned(pixels, 32, 32))
        .tooltip("ClickyAI · Idle")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "activate" => self::activate(app.clone()),
            "pause" => self::pause(app.clone()),
            "settings" => open_settings(app.clone(), None),
            "diagnostics" => open_settings(app.clone(), Some("diagnostics".into())),
            "quit" => shutdown(app.clone()),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[tauri::command]
pub fn enable_wayland_hotkey(app: AppHandle) {
    portal(&app, true);
}
pub fn portal(app: &AppHandle, allow_prompt: bool) {
    if !crate::platform::wayland() {
        return;
    }
    let state = app.state::<Runtime>();
    if let Some(job) = state.portal_job.lock().unwrap().take() {
        job.abort();
    }
    let handle = app.clone();
    let job = tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let result = async {
            let mut child = tokio::process::Command::new(voice::python())
                .arg(voice::script(&handle)?)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|_| "Wayland hotkeys need Python and dbus-next".to_string())?;
            let mut input = child
                .stdin
                .take()
                .ok_or("Shortcut worker input unavailable")?;
            input
                .write_all(
                    format!(
                        "{}\n",
                        serde_json::json!({"operation":"portal","allow_prompt":allow_prompt})
                    )
                    .as_bytes(),
                )
                .await
                .map_err(|e| e.to_string())?;
            drop(input);
            let mut lines = BufReader::new(
                child
                    .stdout
                    .take()
                    .ok_or("Shortcut worker output unavailable")?,
            )
            .lines();
            while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
                if line.len() > 8192 {
                    return Err("Invalid shortcut worker response".into());
                }
                let event: serde_json::Value =
                    serde_json::from_str(&line).map_err(|e| e.to_string())?;
                match event["event"].as_str() {
                    Some("activate") => activate(handle.clone()),
                    Some("ready") => {
                        *handle.state::<Runtime>().hotkey.lock().unwrap() =
                            event["shortcut"].as_str().unwrap_or("Portal").into();
                        *handle.state::<Runtime>().shortcut_error.lock().unwrap() = None;
                    }
                    Some("unavailable" | "error") => {
                        return Err(event["message"]
                            .as_str()
                            .unwrap_or("Wayland shortcuts unavailable; use the tray")
                            .to_string())
                    }
                    _ => return Err("Unknown shortcut worker event".into()),
                }
            }
            Err::<(), String>(
                "Wayland shortcut service disconnected; tray activation remains available".into(),
            )
        }
        .await;
        if let Err(error) = result {
            *handle.state::<Runtime>().shortcut_error.lock().unwrap() = Some(error);
        }
    });
    *state.portal_job.lock().unwrap() = Some(job);
}

pub fn shutdown(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Runtime>();
        state.generation.fetch_add(1, Ordering::SeqCst);
        let job = state.job.lock().unwrap().take();
        let portal = state.portal_job.lock().unwrap().take();
        for job in [job, portal].into_iter().flatten() {
            job.abort();
            let _ = job.await;
        }
        app.exit(0);
    });
}
