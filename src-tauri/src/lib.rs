use tauri::{Manager, Emitter};
use device_query::{DeviceQuery, DeviceState, MouseState};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use std::sync::{Arc, Mutex};
use std::process::{Command, Child};
use std::time::Duration;
use std::thread;
use base64::{Engine as _, engine::general_purpose};

async fn ask_gemini(user_text: &str, x: i32, y: i32) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = std::env::var("GEMINI_API_KEY")?;
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-flash-latest:generateContent?key={}", api_key);
    
    let client = reqwest::Client::new();
    
    let image_bytes = std::fs::read("/tmp/clickyai_vision.png")?;
    let base64_image = general_purpose::STANDARD.encode(&image_bytes);
    
    // We strictly limit Gemini's response length so it fits entirely into the Google Translate TTS engine!
    let prompt = format!("You are an AI desktop tutor. The user's mouse is currently at X: {}, Y: {}. The user typed: '{}'. Look at the screenshot (a 600x600 region around their mouse) to understand their context. Give a brief, helpful, step-by-step answer as if speaking. KEEP IT UNDER 150 CHARACTERS AND DO NOT USE MARKDOWN.", x, y, user_text);
    
    let payload = serde_json::json!({
        "contents": [{
            "parts": [
                { "text": prompt },
                {
                    "inline_data": {
                        "mime_type": "image/png",
                        "data": base64_image
                    }
                }
            ]
        }]
    });
    
    let res = client.post(&url)
        .json(&payload)
        .send()
        .await?;
        
    let json: serde_json::Value = res.json().await?;
    if let Some(text) = json["candidates"][0]["content"]["parts"][0]["text"].as_str() {
        Ok(text.to_string())
    } else {
        Err(format!("Gemini API Error: {}", json.to_string()).into())
    }
}

async fn play_tts(text: &str) {
    let client = reqwest::Client::new();
    // High-quality Google Translate TTS!
    let url = format!("https://translate.google.com/translate_tts?ie=UTF-8&q={}&tl=en&client=tw-ob", urlencoding::encode(text));
    
    if let Ok(res) = client.get(&url).send().await {
        if let Ok(audio_bytes) = res.bytes().await {
            let _ = std::fs::write("/tmp/clickyai_response.mp3", audio_bytes);
            
            // Play the high-quality MP3 using ffplay natively!
            let _ = Command::new("ffplay")
                .arg("-nodisp")
                .arg("-autoexit")
                .arg("/tmp/clickyai_response.mp3")
                .output();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let is_busy = Arc::new(Mutex::new(false));
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let mut busy = is_busy.lock().unwrap();
                        
                        if !*busy {
                            *busy = true;
                            
                            // Turn orb red
                            let js_code = "document.getElementById('ai-companion').classList.add('listening');";
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.eval(js_code);
                            }
                            
                            let device_state = DeviceState::new();
                            let mouse = device_state.get_mouse();
                            let (cursor_x, cursor_y) = mouse.coords;
                            
                            // 1. Capture 600x600 region around mouse instead of full screen
                            if let Ok(screens) = screenshots::Screen::all() {
                                if let Some(screen) = screens.first() {
                                    let mut cap_x = cursor_x - 300;
                                    let mut cap_y = cursor_y - 300;
                                    if cap_x < 0 { cap_x = 0; }
                                    if cap_y < 0 { cap_y = 0; }
                                    
                                    if let Ok(image) = screen.capture_area(cap_x, cap_y, 600, 600) {
                                        let _ = image.save("/tmp/clickyai_vision.png");
                                        println!("600x600 Vision captured around mouse!");
                                    }
                                }
                            }
                            
                            // 2. Ask user for text input since Mic is broken, using a native Linux KDE popup!
                            let kdialog_output = Command::new("kdialog")
                                .arg("--inputbox")
                                .arg("Ask ClickyAI a question about the area under your mouse:")
                                .output();
                                
                            let mut user_text = String::new();
                            if let Ok(output) = kdialog_output {
                                user_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            }
                            
                            // Turn orb blue
                            let js_code = "document.getElementById('ai-companion').classList.remove('listening');";
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.eval(js_code);
                            }
                            
                            if user_text.is_empty() {
                                println!("No text entered, aborting.");
                                *busy = false;
                                return;
                            }
                            
                            // LOAD ENV VARIABLES
                            let _ = dotenvy::dotenv();
                            
                            let busy_clone = is_busy.clone();
                            tauri::async_runtime::spawn(async move {
                                println!("Asking Gemini...");
                                match ask_gemini(&user_text, cursor_x, cursor_y).await {
                                    Ok(response) => {
                                        println!("============================");
                                        println!("GEMINI SAYS: {}", response);
                                        println!("============================");
                                        
                                        println!("Generating High-Quality Speech via Google TTS...");
                                        play_tts(&response).await;
                                    }
                                    Err(e) => println!("Gemini failed: {:?}", e),
                                }
                                
                                *busy_clone.lock().unwrap() = false;
                            });
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            let hotkey = "alt+x".parse::<Shortcut>().unwrap();
            let _ = app.global_shortcut().register(hotkey);

            let main_window = app.get_webview_window("main").unwrap();
            let _ = main_window.set_ignore_cursor_events(true);
            
            thread::spawn(move || {
                let device_state = DeviceState::new();
                loop {
                    let mouse: MouseState = device_state.get_mouse();
                    let (x, y) = mouse.coords;
                    let _ = main_window.set_position(
                        tauri::Position::Physical(tauri::PhysicalPosition::new(x - 5, y - 5))
                    );
                    thread::sleep(Duration::from_millis(16));
                }
            });
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
