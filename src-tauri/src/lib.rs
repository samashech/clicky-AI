mod interaction;
mod perception;
mod platform;
mod provider;
mod session;
mod task;
mod voice;
use session::Phase;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use task::{Status, Task};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

struct FrameCache {
    image: Vec<u8>,
    origin: (f64, f64),
    elements: Vec<task::UIElement>,
}
#[derive(Default)]
struct Runtime {
    task: Mutex<Option<Task>>,
    session: Mutex<session::Session>,
    voice_config: Mutex<voice::Config>,
    voice_error: Mutex<Option<String>>,
    pointer: Mutex<Option<(f64, f64)>>,
    config: Mutex<provider::Config>,
    router: provider::Router,
    generation: AtomicU64,
    job: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    portal_job: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    hotkey: Mutex<String>,
    input_error: Mutex<Option<String>>,
    shortcut_error: Mutex<Option<String>>,
    model_calls: AtomicU64,
    frame_cache: Mutex<Option<FrameCache>>,
}
fn publish(app: &AppHandle) {
    let value = app.state::<Runtime>().task.lock().unwrap().clone();
    let _ = app.emit("task-state", value);
}
fn control(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("control") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
fn hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}
fn stop(app: &AppHandle) {
    let state = app.state::<Runtime>();
    state.generation.fetch_add(1, Ordering::SeqCst);
    if let Some(job) = state.job.lock().unwrap().take() {
        job.abort();
        eprintln!(
            "{}",
            serde_json::json!({"event":"inference_cancelled","classification":provider::Error::Cancelled})
        );
    }
    hide_overlay(app);
    *state.frame_cache.lock().unwrap() = None;
    state.router.clear();
}

#[tauri::command]
fn cancel_task(app: AppHandle) {
    stop(&app);
    if let Some(task) = app.state::<Runtime>().task.lock().unwrap().as_mut() {
        task.status = Status::Cancelled;
    }
    publish(&app);
    interaction::idle(&app);
}
#[tauri::command]
fn diagnostics(app: AppHandle) -> serde_json::Value {
    let state = app.state::<Runtime>();
    let mut result = platform::diagnostics();
    result["input_error"] = serde_json::json!(*state.input_error.lock().unwrap());
    result["shortcut_error"] = serde_json::json!(*state.shortcut_error.lock().unwrap());
    result["hotkey"] = serde_json::json!(*state.hotkey.lock().unwrap());
    result["voice_error"] = serde_json::json!(*state.voice_error.lock().unwrap());
    result["runtime"] = serde_json::json!(*state.session.lock().unwrap());
    result
}
#[tauri::command]
fn configure(app: AppHandle, config: provider::Config, hotkey: String) -> Result<(), String> {
    if hotkey.len() > 80 || hotkey.eq_ignore_ascii_case("Escape") {
        return Err("Invalid shortcut".into());
    }
    let state = app.state::<Runtime>();
    let old = state.hotkey.lock().unwrap().clone();
    if old != hotkey && !platform::wayland() {
        app.global_shortcut()
            .register(hotkey.as_str())
            .map_err(|e| e.to_string())?;
        if !old.is_empty() {
            let _ = app.global_shortcut().unregister(old.as_str());
        }
        *state.hotkey.lock().unwrap() = hotkey;
        *state.shortcut_error.lock().unwrap() = None;
    }
    *state.config.lock().unwrap() = config;
    Ok(())
}
#[tauri::command]
fn start_task(app: AppHandle, goal: String, mode: String) -> Result<(), String> {
    if !["guide", "explain", "teach"].contains(&mode.as_str()) {
        return Err("Invalid guidance mode".into());
    }
    cancel_task(app.clone());
    interaction::phase(&app, Phase::Activating, "Starting…");
    interaction::phase(&app, Phase::Thinking, "Thinking…");
    if let Err(error) = interaction::begin(&app, &goal, &mode) {
        fail(&app, error.clone());
        return Err(error);
    }
    if let Some(w) = app.get_webview_window("control") {
        let _ = w.hide();
    }
    schedule(&app);
    Ok(())
}
fn schedule(app: &AppHandle) {
    let handle = app.clone();
    let job = tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        ground(handle).await;
    });
    *app.state::<Runtime>().job.lock().unwrap() = Some(job);
}
async fn ground(app: AppHandle) {
    let state = app.state::<Runtime>();
    let generation = state.generation.load(Ordering::SeqCst);
    let (instruction, hint, needs_plan) = {
        let mut guard = state.task.lock().unwrap();
        let Some(task) = guard.as_mut() else { return };
        if task.current >= task.steps.len() {
            return;
        }
        task.status = Status::Looking;
        task.steps[task.current].status = Status::Looking;
        (
            task.steps[task.current].instruction.clone(),
            task.steps[task.current].target_hint.clone(),
            task.needs_plan,
        )
    };
    publish(&app);
    hide_overlay(&app);
    interaction::phase(&app, Phase::AnalyzingScreen, "Looking at the application…");
    let started = std::time::Instant::now();
    let result=async {
        let native_hint=hint.clone();
        let native=tauri::async_runtime::spawn_blocking(move||if needs_plan{None}else{perception::native(&native_hint)}).await.map_err(|e|e.to_string())?;
        if let Some(target)=native{return Ok(target);}
        let pointer=state.pointer.lock().unwrap().ok_or("Move the pointer over the application and try again")?;
        if let Some(hud)=app.get_webview_window("indicator"){let _=hud.hide();}
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        let (image,origin)=tauri::async_runtime::spawn_blocking(move||perception::capture(pointer)).await.map_err(|e|e.to_string())??;
        interaction::phase(&app,Phase::AnalyzingScreen,"Reading visible controls…");
        let script=if cfg!(debug_assertions){std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../omni_server/main.py")}else{app.path().resource_dir().map_err(|e|e.to_string())?.join("perception/main.py")};
        let cached={let cache=state.frame_cache.lock().unwrap();cache.as_ref().filter(|c|c.origin==origin && c.image==image).map(|c|c.elements.clone())};
        let elements=if let Some(elements)=cached {elements} else {
            let elements=perception::ocr(script,image.clone(),origin).await?;
            *state.frame_cache.lock().unwrap()=Some(FrameCache{image,origin,elements:elements.clone()});elements
        };
        let config=state.config.lock().unwrap().clone();
        let hint=if needs_plan {
            interaction::phase(&app,Phase::Planning,"Planning the steps…");
            if config.kind==provider::Kind::Disabled{return Err("For a tutorial, choose a reasoning model in Settings. Without a model, ask me to find a named control.".into());}
            if state.model_calls.fetch_add(1,Ordering::SeqCst)>=4{return Err("Task reasoning budget reached".into());}
            match state.router.plan(&config,&instruction,&elements).await.map_err(|e|format!("Planning unavailable ({e:?}). Try asking for a specific control."))? {
                provider::Decision::Plan{steps}=>{let mut task=state.task.lock().unwrap();let task=task.as_mut().ok_or("Task missing")?;task.apply_plan(steps)?;task.steps[0].target_hint.clone()},
                provider::Decision::AskUser{question}=>return Err(question),_=>return Err("Invalid task plan".into())
            }
        }else{hint};
        if let Some(target)=task::exact_match(&hint,&elements){return Ok(target.clone());}
        interaction::phase(&app,Phase::Planning,"Finding the right control…");
        let config=state.config.lock().unwrap().clone();
        if config.kind==provider::Kind::Disabled{return Err("No unique visible label found. Use the exact control label, or configure a reasoning provider.".into());}
        if state.model_calls.fetch_add(1,Ordering::SeqCst)>=4{return Err("Task reasoning budget reached. Refine the request.".into());}
        match state.router.select(&config,&instruction,&elements).await.map_err(|e|format!("Reasoning unavailable ({e:?}). Local matching remains available; try an exact visible label."))? {
            provider::Decision::Highlight{target_id,confidence,..} if confidence>=0.65=>elements.into_iter().find(|e|e.id==target_id).ok_or("Selected target disappeared".into()),
            provider::Decision::AskUser{question}=>Err(question),
            _=>Err("I'm not sure which control you mean. Use its visible label.".into())
        }
    }.await;
    if generation != state.generation.load(Ordering::SeqCst) {
        return;
    }
    match result {
        Ok(target) => {
            let show = (|| -> Result<task::Step, String> {
                let w = app
                    .get_webview_window("main")
                    .ok_or("Overlay unavailable")?;
                let monitor = w
                    .monitor_from_point(
                        target.bounds.x + target.bounds.width / 2.,
                        target.bounds.y + target.bounds.height / 2.,
                    )
                    .map_err(|e| e.to_string())?
                    .ok_or("Target monitor unavailable")?;
                w.set_fullscreen(false).map_err(|e| e.to_string())?;
                w.set_position(*monitor.position())
                    .map_err(|e| e.to_string())?;
                w.set_size(*monitor.size()).map_err(|e| e.to_string())?;
                let rect = target.bounds.logical(
                    (monitor.position().x as f64, monitor.position().y as f64),
                    monitor.scale_factor(),
                );
                let step = {
                    let mut guard = state.task.lock().unwrap();
                    let task = guard.as_mut().ok_or("Task missing")?;
                    task.select(target)?;
                    task.steps[task.current].clone()
                };
                w.show().map_err(|e| e.to_string())?;
                w.set_ignore_cursor_events(true)
                    .map_err(|e| e.to_string())?;
                w.set_focusable(false).map_err(|e| e.to_string())?;
                app.emit_to("main","draw-spotlight",serde_json::json!({"bounds":rect,"step":step,"origin":[monitor.position().x,monitor.position().y],"scale":monitor.scale_factor()})).map_err(|e|e.to_string())?;
                Ok(step)
            })();
            match show {
                Err(error) => fail(&app, error),
                Ok(step) => {
                    interaction::phase(&app, Phase::Guiding, "Showing you…");
                    interaction::phase(&app, Phase::Speaking, step.instruction.clone());
                    let config = state.voice_config.lock().unwrap().synthesis.clone();
                    let spoken = voice::speak(&app, &step.instruction, config).await;
                    let message = if let Err(error) = spoken {
                        *state.voice_error.lock().unwrap() = Some(error);
                        "Speech unavailable · follow the highlight"
                    } else {
                        "Your turn · Alt+X to stop"
                    };
                    if generation == state.generation.load(Ordering::SeqCst) {
                        interaction::phase(&app, Phase::WaitingForUser, message);
                    }
                }
            }
        }
        Err(error) => fail(&app, error),
    }
    eprintln!(
        "{}",
        serde_json::json!({"event":"grounding_finished","task_id":generation,"elapsed_ms":started.elapsed().as_millis()})
    );
    publish(&app);
}
fn fail(app: &AppHandle, message: String) {
    if let Some(task) = app.state::<Runtime>().task.lock().unwrap().as_mut() {
        task.status = Status::Uncertain;
        task.message = message.clone();
    }
    hide_overlay(app);
    interaction::error(app, message);
}

pub fn run() {
    let builder = tauri::Builder::default().manage(Runtime::default());
    let builder = if !platform::wayland() {
        builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if shortcut.key == tauri_plugin_global_shortcut::Code::Escape {
                            cancel_task(app.clone());
                        } else {
                            interaction::activate(app.clone());
                        }
                    }
                })
                .build(),
        )
    } else {
        builder
    };
    builder
        .setup(|app| {
            interaction::tray(app)?;
            interaction::portal(app.handle(), false);
            let state = app.state::<Runtime>();
            if !platform::wayland() {
                match app.global_shortcut().register("Alt+X") {
                    Ok(()) => *state.hotkey.lock().unwrap() = "Alt+X".into(),
                    Err(_) => {
                        *state.shortcut_error.lock().unwrap() = Some(
                            "Global shortcut registration failed; configure another shortcut"
                                .into(),
                        )
                    }
                }
            }
            if !platform::wayland() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let events = handle.clone();
                    let mut last = std::time::Instant::now();
                    let result = rdev::listen(move |event| {
                        let state = events.state::<Runtime>();
                        match event.event_type {
                            rdev::EventType::MouseMove { x, y } => {
                                *state.pointer.lock().unwrap() = Some((x, y));
                                let active = state
                                    .task
                                    .lock()
                                    .unwrap()
                                    .as_ref()
                                    .is_some_and(|t| t.status == Status::Waiting);
                                if active && last.elapsed() >= std::time::Duration::from_millis(16)
                                {
                                    let _ = events.emit_to(
                                        "main",
                                        "mouse-move",
                                        serde_json::json!({"x":x,"y":y}),
                                    );
                                    last = std::time::Instant::now();
                                }
                            }
                            rdev::EventType::ButtonPress(rdev::Button::Left) => {
                                let pointer = *state.pointer.lock().unwrap();
                                if let Some((x, y)) = pointer {
                                    let advanced = {
                                        let mut task = state.task.lock().unwrap();
                                        task.as_mut().is_some_and(|t| t.click(x, y))
                                    };
                                    if advanced {
                                        interaction::phase(
                                            &events,
                                            Phase::Verifying,
                                            "Checking the click…",
                                        );
                                        if let Some(job) = state.job.lock().unwrap().take() {
                                            job.abort();
                                        }
                                        hide_overlay(&events);
                                        publish(&events);
                                        let complete = state
                                            .task
                                            .lock()
                                            .unwrap()
                                            .as_ref()
                                            .is_some_and(|t| t.status == Status::Complete);
                                        if complete {
                                            interaction::finish(&events);
                                        } else {
                                            let app = events.clone();
                                            let job = tauri::async_runtime::spawn(async move {
                                                tokio::time::sleep(
                                                    std::time::Duration::from_millis(500),
                                                )
                                                .await;
                                                ground(app).await;
                                            });
                                            *state.job.lock().unwrap() = Some(job);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    });
                    if result.is_err() {
                        *handle.state::<Runtime>().input_error.lock().unwrap() =
                            Some("Global pointer hook failed".into());
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_task,
            cancel_task,
            diagnostics,
            configure,
            interaction::activate,
            interaction::pause,
            interaction::open_settings,
            interaction::runtime_state,
            interaction::configure_voice,
            interaction::voice_settings,
            interaction::voice_diagnostics,
            interaction::enable_wayland_hotkey
        ])
        .run(tauri::generate_context!())
        .expect("ClickyAI native initialization failed");
}
