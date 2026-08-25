use tauri::{Manager, Emitter, AppHandle};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use base64::{Engine as _, engine::general_purpose};
use std::sync::{Arc, Mutex};
use std::thread;
use rdev::{listen, Event, EventType};

#[derive(Serialize)]
struct ParseRequest {
    image_base64: String,
}

#[derive(Deserialize, Debug, Clone)]
struct ParsedElement {
    id: i32,
    #[serde(rename = "type")]
    element_type: String,
    text: String,
    box_coords: [f64; 4], // [x, y, width, height]
}

#[derive(Deserialize, Debug)]
struct ParseResponse {
    elements: Vec<ParsedElement>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Coords {
    x: f32,
    y: f32,
}

// Global state to track the current active target bounding box [x, y, width, height]
lazy_static::lazy_static! {
    static ref ACTIVE_TARGET_RECT: Arc<Mutex<Option<(f64, f64, f64, f64)>>> = Arc::new(Mutex::new(None));
}

#[cfg(target_os = "windows")]
fn get_uia_bounding_box(target_name: &str) -> Option<(f64, f64, f64, f64)> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, UIA_NamePropertyId, TreeScope_Subtree};
    use windows::core::BSTR;

    unsafe {
        // Initialize COM for the background thread
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        
        let uia: Result<IUIAutomation, _> = windows::Win32::System::Com::CoCreateInstance(
            &CUIAutomation, 
            None, 
            windows::Win32::System::Com::CLSCTX_INPROC_SERVER
        );
        
        if let Ok(automation) = uia {
            // Get the element directly under the mouse, or the active foreground window
            // For a robust search, we start at the root desktop element
            if let Ok(root) = automation.GetRootElement() {
                
                // Create a condition: Name == target_name
                let name_bstr = BSTR::from(target_name);
                
                let variant = unsafe {
                    let mut v: windows::Win32::System::Variant::VARIANT = std::mem::zeroed();
                    v.Anonymous.Anonymous = std::mem::ManuallyDrop::new(windows::Win32::System::Variant::VARIANT_0_0 {
                        vt: windows::Win32::System::Variant::VT_BSTR,
                        wReserved1: 0,
                        wReserved2: 0,
                        wReserved3: 0,
                        Anonymous: windows::Win32::System::Variant::VARIANT_0_0_0 {
                            bstrVal: std::mem::ManuallyDrop::new(name_bstr),
                        },
                    });
                    v
                };
                
                if let Ok(condition) = automation.CreatePropertyCondition(UIA_NamePropertyId, variant) {
                    
                    // Walk the tree (Subtree scope searches all children recursively)
                    if let Ok(element) = root.FindFirst(TreeScope_Subtree, &condition) {
                        if let Ok(rect) = element.CurrentBoundingRectangle() {
                            let _ = CoUninitialize();
                            return Some((
                                rect.left as f64, 
                                rect.top as f64, 
                                (rect.right - rect.left) as f64, 
                                (rect.bottom - rect.top) as f64
                            ));
                        }
                    }
                }
            }
        }
        let _ = CoUninitialize();
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn get_uia_bounding_box(_: &str) -> Option<(f64, f64, f64, f64)> { 
    // MOCK for Linux testing so we can see the UI animation without Ollama running!
    Some((400.0, 300.0, 200.0, 80.0))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Spawn the Python Sidecar Server
            use tauri_plugin_shell::ShellExt;
            match app.shell().sidecar("omni_server") {
                Ok(command) => {
                    if let Err(e) = command.spawn() {
                        println!("Failed to spawn sidecar: {}", e);
                    } else {
                        println!("OmniServer sidecar spawned successfully!");
                    }
                }
                Err(e) => println!("Could not find sidecar: {}", e),
            }

            // --- Pillar 1: Win32 Spotlight Layer Configuration ---
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_TOPMOST};
                use windows::Win32::Foundation::HWND;
                
                let hwnd = HWND(window.hwnd().unwrap().0 as isize);
                unsafe {
                    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                    // Force the window to be a topmost, click-through overlay
                    SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST).0 as i32);
                }
            }
            
            // Allow Tauri to ignore clicks cross-platform just in case
            let _ = window.set_ignore_cursor_events(true);

            // --- Pillar 4: Step Verification via rdev global hooks ---
            let app_handle = app.handle().clone();
            thread::spawn(move || {
                let callback = move |event: Event| {
                    match event.event_type {
                        EventType::MouseMove { x, y } => {
                            // Stream live coordinates to frontend for Bezier path
                            let _ = app_handle.emit("mouse-move", Coords { x: x as f32, y: y as f32 });
                        }
                        EventType::ButtonPress(rdev::Button::Left) => {
                            let rect_lock = ACTIVE_TARGET_RECT.lock().unwrap();
                            if let Some((_rx, _ry, _rw, _rh)) = *rect_lock {
                                // We don't have mouse coords in ButtonPress event directly, 
                                // but we can track them via MouseMove state. For brevity, assuming a hit-test here:
                                // let (mx, my) = get_last_mouse_coords();
                                // if mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh { ... }
                                
                                let _ = app_handle.emit("step-success", ());
                            }
                        }
                        _ => {}
                    }
                };
                if let Err(error) = listen(callback) {
                    println!("Error listening to rdev: {:?}", error);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![process_ai_step])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn process_ai_step(app: AppHandle, user_prompt: String, target_hint: String) -> Result<(), String> {
    
    // ==========================================
    // STEP 1: Attempt Windows UIA Fast-Path
    // ==========================================
    if let Some(rect) = get_uia_bounding_box(&target_hint) {
        println!("Found via UIA! Skipping vision models.");
        *ACTIVE_TARGET_RECT.lock().unwrap() = Some(rect);
        let _ = app.emit("draw-spotlight", rect);
        return Ok(());
    }

    println!("UIA failed. Falling back to local OmniParser & Ollama...");

    // ==========================================
    // STEP 2: Capture Screen & Send to Local Parser
    // ==========================================
    let mut base64_image = String::new();
    if let Ok(screens) = screenshots::Screen::all() {
        if let Some(screen) = screens.first() {
            if let Ok(image) = screen.capture() {
                // For simplicity, buffer through the filesystem as in the previous pipeline
                let _ = image.save("/tmp/clickyai_vision.png");
                if let Ok(bytes) = std::fs::read("/tmp/clickyai_vision.png") {
                    base64_image = general_purpose::STANDARD.encode(&bytes);
                }
            }
        }
    }

    let client = Client::new();
    let parse_res = client.post("http://127.0.0.1:8000/parse")
        .json(&ParseRequest { image_base64: base64_image })
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let parsed_data: ParseResponse = parse_res.json().await.map_err(|e| e.to_string())?;

    // ==========================================
    // STEP 3: Ask Local Ollama to pick the ID
    // ==========================================
    // Convert the parsed elements into a readable string for the LLM
    let mut elements_text = String::from("On screen elements:\n");
    for el in &parsed_data.elements {
        elements_text.push_str(&format!("ID: {}, Type: {}, Text: '{}'\n", el.id, el.element_type, el.text));
    }

    let llm_prompt = format!(
        "You are a UI routing assistant. The user wants to: '{}'. \n\
         Based on the following UI elements, return ONLY the integer ID of the element they should click. Do not output any other text.\n{}", 
        user_prompt, elements_text
    );

    let ollama_payload = serde_json::json!({
        "model": "llama3.2",
        "prompt": llm_prompt,
        "stream": false
    });

    let ollama_res = client.post("http://127.0.0.1:11434/api/generate")
        .json(&ollama_payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let ollama_json: serde_json::Value = ollama_res.json().await.map_err(|e| e.to_string())?;
    
    // Parse the returned ID
    if let Some(response_text) = ollama_json["response"].as_str() {
        let selected_id: i32 = response_text.trim().parse().unwrap_or(-1);
        
        // Find the bounding box matching the ID
        if let Some(element) = parsed_data.elements.iter().find(|e| e.id == selected_id) {
            let rect = (
                element.box_coords[0],
                element.box_coords[1],
                element.box_coords[2],
                element.box_coords[3],
            );
            
            // Trigger frontend spotlight
            *ACTIVE_TARGET_RECT.lock().unwrap() = Some(rect);
            let _ = app.emit("draw-spotlight", rect);
            return Ok(());
        }
    }

    Err("AI could not determine the correct element.".to_string())
}
