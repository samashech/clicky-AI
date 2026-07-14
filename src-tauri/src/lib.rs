use tauri::{Manager, Emitter, AppHandle};
use device_query::{DeviceQuery, DeviceState, MouseState};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use std::sync::{Arc, Mutex};
use std::process::Command;
use std::time::Duration;
use std::thread;
use base64::{Engine as _, engine::general_purpose};

async fn run_ai_pipeline(app: AppHandle, user_text: Option<String>, has_audio: bool, x: i32, y: i32, is_busy: Arc<Mutex<bool>>) {
    // 1. Capture Full Vision
    if let Ok(screens) = screenshots::Screen::all() {
        if let Some(screen) = screens.first() {
            if let Ok(image) = screen.capture() {
                let _ = image.save("/tmp/clickyai_vision.png");
            }
        }
    }
    
    let api_key = match std::env::var("GEMINI_API_KEY") {
        Ok(k) => k,
        Err(_) => {
            *is_busy.lock().unwrap() = false;
            return;
        }
    };
    
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}", api_key);
    let client = reqwest::Client::new();
    
    let image_bytes = std::fs::read("/tmp/clickyai_vision.png").unwrap_or_default();
    let base64_image = general_purpose::STANDARD.encode(&image_bytes);
    
    let mut parts = vec![];
    
    // Core Instructions
    let prompt = format!("You are a hyper-intelligent desktop AI assistant. The user's mouse cursor is currently located at exact screen coordinates X: {}, Y: {}. Look at the attached full-screen screenshot. If the user asks a question, answer it by looking precisely at what their mouse is pointing at! Keep your response brief, helpful, and under 150 characters as it will be spoken out loud. Do not use markdown.", x, y);
    parts.push(serde_json::json!({ "text": prompt }));
    
    // Add Text Input if any
    if let Some(text) = user_text {
        parts.push(serde_json::json!({ "text": format!("The user typed this question: {}", text) }));
    }
    
    // Add Image
    parts.push(serde_json::json!({
        "inline_data": {
            "mime_type": "image/png",
            "data": base64_image
        }
    }));
    
    // Add Audio Input if any
    if has_audio {
        if let Ok(audio_bytes) = std::fs::read("/tmp/clickyai_audio.wav") {
            let base64_audio = general_purpose::STANDARD.encode(&audio_bytes);
            parts.push(serde_json::json!({
                "inline_data": {
                    "mime_type": "audio/wav",
                    "data": base64_audio
                }
            }));
        }
    }
    
    let payload = serde_json::json!({
        "contents": [{
            "parts": parts
        }]
    });
    
    let mut response_text = String::new();
    if let Ok(res) = client.post(&url).json(&payload).send().await {
        if let Ok(json) = res.json::<serde_json::Value>().await {
            if let Some(text) = json["candidates"][0]["content"]["parts"][0]["text"].as_str() {
                response_text = text.to_string();
            } else {
                println!("Gemini Error: {}", json.to_string());
            }
        }
    }
    
    if response_text.is_empty() {
        if let Some(w) = app.get_webview_window("main") { 
            let _ = w.eval("document.getElementById('ai-companion').className = 'orb';"); 
        }
        *is_busy.lock().unwrap() = false;
        return;
    }
    
    println!("============================");
    println!("GEMINI SAYS: {}", response_text);
    println!("============================");
    
    if let Some(w) = app.get_webview_window("main") { 
        let _ = w.eval("document.getElementById('ai-companion').className = 'orb';"); 
    }
    
    // 4. TTS
    let tts_url = format!("https://translate.google.com/translate_tts?ie=UTF-8&q={}&tl=en&client=tw-ob", urlencoding::encode(response_text.trim()));
    if let Ok(res) = client.get(&tts_url).send().await {
        if let Ok(audio_bytes) = res.bytes().await {
            let _ = std::fs::write("/tmp/clickyai_response.mp3", audio_bytes);
            let _ = Command::new("ffplay").arg("-nodisp").arg("-autoexit").arg("/tmp/clickyai_response.mp3").output();
        }
    }
    
    *is_busy.lock().unwrap() = false;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let is_busy = Arc::new(Mutex::new(false));
    let is_busy_shortcut = is_busy.clone();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let mut busy = is_busy_shortcut.lock().unwrap();
                        if !*busy {
                            *busy = true;
                            
                            let device_state = DeviceState::new();
                            let mouse = device_state.get_mouse();
                            let (cursor_x, cursor_y) = mouse.coords;
                            
                            // 1. Pop up a Menu to choose input type
                            let kdialog_choice = Command::new("kdialog")
                                .arg("--menu")
                                .arg("How would you like to ask ClickyAI?")
                                .arg("type")
                                .arg("Type Text")
                                .arg("speak")
                                .arg("Record Audio")
                                .output();
                                
                            let mut choice = String::new();
                            if let Ok(output) = kdialog_choice {
                                choice = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            }
                            
                            if choice.is_empty() {
                                *busy = false;
                                return;
                            }
                            
                            let mut user_text = None;
                            let mut has_audio = false;
                            
                            // 2. Handle specific input type
                            if choice == "type" {
                                let kdialog_input = Command::new("kdialog")
                                    .arg("--inputbox")
                                    .arg("Type your question:")
                                    .output();
                                    
                                if let Ok(output) = kdialog_input {
                                    let txt = String::from_utf8_lossy(&output.stdout).trim().to_string();
                                    if txt.is_empty() {
                                        *busy = false;
                                        return;
                                    }
                                    user_text = Some(txt);
                                } else {
                                    *busy = false;
                                    return;
                                }
                            } else if choice == "speak" {
                                // Start recording audio in background
                                let mut arecord = Command::new("arecord")
                                    .arg("-f")
                                    .arg("S16_LE")
                                    .arg("-r")
                                    .arg("16000")
                                    .arg("-c")
                                    .arg("1")
                                    .arg("/tmp/clickyai_audio.wav")
                                    .spawn()
                                    .expect("Failed to start arecord");
                                    
                                // Block the UI with a native popup to stop recording
                                let _ = Command::new("kdialog")
                                    .arg("--msgbox")
                                    .arg("🔴 Recording your voice...\n\nPress OK to stop recording and send to AI!")
                                    .output();
                                    
                                // Kill arecord when the user dismisses the box!
                                let _ = arecord.kill();
                                let _ = arecord.wait();
                                has_audio = true;
                            }
                            
                            // Visual indicator for listening/processing
                            let js_code = "document.getElementById('ai-companion').className = 'orb listening';";
                            if let Some(window) = app.get_webview_window("main") { let _ = window.eval(js_code); }
                            
                            let _ = dotenvy::dotenv();
                            
                            let app_clone = app.clone();
                            let busy_clone = is_busy_shortcut.clone();
                            
                            tauri::async_runtime::spawn(async move {
                                println!("Starting AI Pipeline (Single Shot)...");
                                run_ai_pipeline(app_clone, user_text, has_audio, cursor_x, cursor_y, busy_clone).await;
                            });
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let hotkey = "alt+x".parse::<Shortcut>().unwrap();
            let _ = app.global_shortcut().register(hotkey);

            if let Some(w) = app.get_webview_window("main") { let _ = w.set_ignore_cursor_events(true); }
            
            let app_handle_thread = app.handle().clone();
            
            // Minimal loop just to animate the orb to follow the cursor!
            thread::spawn(move || {
                let device_state = DeviceState::new();
                loop {
                    let mouse: MouseState = device_state.get_mouse();
                    let (x, y) = mouse.coords;
                    
                    if let Some(w) = app_handle_thread.get_webview_window("main") {
                        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x - 5, y - 5)));
                    }
                    
                    thread::sleep(Duration::from_millis(16));
                }
            });
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
