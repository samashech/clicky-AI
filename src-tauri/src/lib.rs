use tauri::{Manager, Emitter, AppHandle};
use device_query::{DeviceQuery, DeviceState, MouseState};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use std::sync::{Arc, Mutex};
use std::process::Command;
use std::time::Duration;
use std::thread;
use base64::{Engine as _, engine::general_purpose};

async fn run_ai_pipeline(app: AppHandle, user_text: Option<String>, x: i32, y: i32, is_guiding: Arc<Mutex<bool>>, is_busy: Arc<Mutex<bool>>) {
    // 1. Capture Full Vision
    if let Ok(screens) = screenshots::Screen::all() {
        if let Some(screen) = screens.first() {
            if let Ok(image) = screen.capture() {
                let _ = image.save("/tmp/clickyai_vision.png");
            }
        }
    }
    
    // 2. Prepare Prompt (Full screen context for max accuracy)
    let prompt = if let Some(text) = user_text {
        format!("You are an AI desktop tutor. The user's mouse is at X: {}, Y: {}. They typed: '{}'. Look at the full screen screenshot. If it requires multiple steps, give ONLY THE VERY FIRST STEP, and end your response EXACTLY with [GUIDE_MODE_ON]. If it is a simple question requiring no clicks, answer it and end with [GUIDE_MODE_OFF]. Keep it under 150 chars. IMPORTANT: If you want the user to click something, you MUST output the exact ABSOLUTE pixel coordinates of the target element based on the full screen image. Output these coordinates at the very end of your response in the exact format: [X, Y]. Do not use markdown.", x, y, text)
    } else {
        format!("You are in Guide Mode. The user just clicked their mouse at X: {}, Y: {}. Look at the new full screen screenshot. Did they perform the previous step correctly? If yes, give the NEXT step and end with [GUIDE_MODE_ON]. If no, correct them and end with [GUIDE_MODE_ON]. If the task is finished, congratulate them and end with [GUIDE_MODE_OFF]. Keep it under 150 chars. IMPORTANT: If you want the user to click something, you MUST output the exact ABSOLUTE pixel coordinates of the target element based on the full screen image. Output these coordinates at the very end of your response in the exact format: [X, Y]. Do not use markdown.", x, y)
    };
    
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
    
    let payload = serde_json::json!({
        "contents": [{
            "parts": [
                { "text": prompt },
                { "inline_data": { "mime_type": "image/png", "data": base64_image } }
            ]
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
        *is_guiding.lock().unwrap() = false;
        if let Some(w) = app.get_webview_window("main") { 
            let _ = w.eval("document.getElementById('ai-companion').className = 'orb';"); 
        }
        if let Some(w) = app.get_webview_window("highlight") { let _ = w.hide(); }
        *is_busy.lock().unwrap() = false;
        return;
    }
    
    // Parse Coordinates for Highlight Window [X, Y]
    let mut highlight_coords = None;
    if let Some(start) = response_text.rfind('[') {
        if let Some(end) = response_text[start..].find(']') {
            let coords_str = &response_text[start+1 .. start+end];
            let parts: Vec<&str> = coords_str.split(',').collect();
            if parts.len() == 2 {
                if let (Ok(hx), Ok(hy)) = (parts[0].trim().parse::<i32>(), parts[1].trim().parse::<i32>()) {
                    highlight_coords = Some((hx, hy));
                    response_text = response_text[..start].trim().to_string();
                }
            }
        }
    }
    
    println!("============================");
    println!("GEMINI SAYS: {}", response_text);
    println!("HIGHLIGHT COORDS: {:?}", highlight_coords);
    println!("============================");
    
    if let Some((abs_x, abs_y)) = highlight_coords {
        // Absolute Screen Position (Image is now full screen, so coords are already absolute!)
        if let Some(w) = app.get_webview_window("highlight") {
            let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(abs_x - 30, abs_y - 30)));
            let _ = w.show();
        }
    } else {
        if let Some(w) = app.get_webview_window("highlight") { let _ = w.hide(); }
    }
    
    // 3. Handle Guide Mode State
    let mut turn_guide_on = false;
    if response_text.contains("[GUIDE_MODE_ON]") {
        turn_guide_on = true;
        response_text = response_text.replace("[GUIDE_MODE_ON]", "");
    }
    if response_text.contains("[GUIDE_MODE_OFF]") {
        turn_guide_on = false;
        response_text = response_text.replace("[GUIDE_MODE_OFF]", "");
    }
    
    *is_guiding.lock().unwrap() = turn_guide_on;
    
    if turn_guide_on {
        let js = "document.getElementById('ai-companion').className = 'orb guiding';";
        if let Some(w) = app.get_webview_window("main") { let _ = w.eval(js); }
    } else {
        let js = "document.getElementById('ai-companion').className = 'orb';";
        if let Some(w) = app.get_webview_window("main") { let _ = w.eval(js); }
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
    let is_guiding = Arc::new(Mutex::new(false));
    
    let is_busy_shortcut = is_busy.clone();
    let is_guiding_shortcut = is_guiding.clone();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let mut busy = is_busy_shortcut.lock().unwrap();
                        if !*busy {
                            *busy = true;
                            
                            let js_code = "document.getElementById('ai-companion').className = 'orb listening';";
                            if let Some(window) = app.get_webview_window("main") { let _ = window.eval(js_code); }
                            
                            let device_state = DeviceState::new();
                            let mouse = device_state.get_mouse();
                            let (cursor_x, cursor_y) = mouse.coords;
                            
                            let kdialog_output = Command::new("kdialog")
                                .arg("--inputbox")
                                .arg("Ask ClickyAI a question about your screen:")
                                .output();
                                
                            let mut user_text = String::new();
                            if let Ok(output) = kdialog_output {
                                user_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                            }
                            
                            if user_text.is_empty() {
                                println!("No text entered, aborting.");
                                let js = "document.getElementById('ai-companion').className = 'orb';";
                                if let Some(window) = app.get_webview_window("main") { let _ = window.eval(js); }
                                *busy = false;
                                return;
                            }
                            
                            let _ = dotenvy::dotenv();
                            
                            let app_clone = app.clone();
                            let guide_clone = is_guiding_shortcut.clone();
                            let busy_clone = is_busy_shortcut.clone();
                            
                            tauri::async_runtime::spawn(async move {
                                println!("Starting AI Pipeline (Full Screen Mode)...");
                                run_ai_pipeline(app_clone, Some(user_text), cursor_x, cursor_y, guide_clone, busy_clone).await;
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
            if let Some(w) = app.get_webview_window("highlight") { let _ = w.set_ignore_cursor_events(true); }
            
            let is_busy_thread = is_busy.clone();
            let is_guiding_thread = is_guiding.clone();
            let app_handle_thread = app.handle().clone();
            
            thread::spawn(move || {
                let device_state = DeviceState::new();
                let mut was_left_pressed = false;
                
                loop {
                    let mouse: MouseState = device_state.get_mouse();
                    let (x, y) = mouse.coords;
                    
                    if let Some(w) = app_handle_thread.get_webview_window("main") {
                        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x - 5, y - 5)));
                    }
                    
                    let left_pressed = mouse.button_pressed.get(1).copied().unwrap_or(false);
                    
                    if left_pressed && !was_left_pressed {
                        was_left_pressed = true;
                    } else if !left_pressed && was_left_pressed {
                        was_left_pressed = false;
                        
                        let guiding = *is_guiding_thread.lock().unwrap();
                        let mut busy = is_busy_thread.lock().unwrap();
                        
                        if guiding && !*busy {
                            *busy = true;
                            
                            if let Some(w) = app_handle_thread.get_webview_window("highlight") { let _ = w.hide(); }
                            if let Some(w) = app_handle_thread.get_webview_window("main") {
                                let _ = w.eval("document.getElementById('ai-companion').className = 'orb listening';");
                            }
                            
                            thread::sleep(Duration::from_millis(500));
                            
                            let _ = dotenvy::dotenv();
                            let app_clone = app_handle_thread.clone();
                            let guide_clone = is_guiding_thread.clone();
                            let busy_clone = is_busy_thread.clone();
                            
                            println!("Mouse Click Detected! Continuing Guide Mode...");
                            tauri::async_runtime::spawn(async move {
                                run_ai_pipeline(app_clone, None, x, y, guide_clone, busy_clone).await;
                            });
                        }
                    }
                    
                    thread::sleep(Duration::from_millis(16));
                }
            });
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
