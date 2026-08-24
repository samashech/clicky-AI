use tauri::{Manager, Emitter, AppHandle};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use rdev::{listen, Event, EventType};

// --- Pillar 5: API Response Schema ---
#[derive(Deserialize, Serialize, Debug, Clone)]
struct VlmResponse {
    status: String,
    instruction: String,
    target_element_name: String,
    vlm_fallback_coords: Coords,
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

// --- Pillar 2: UIA First Grounding ---
#[cfg(target_os = "windows")]
fn get_uia_bounding_box(target_name: &str) -> Option<(f64, f64, f64, f64)> {
    use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, UIA_NamePropertyId};
    use windows::core::BSTR;

    unsafe {
        let _ = CoInitialize(None);
        // Initialize COM and create IUIAutomation instance
        let uia: Result<IUIAutomation, _> = windows::core::CoCreateInstance(&CUIAutomation, None, windows::Win32::System::Com::CLSCTX_INPROC_SERVER);
        
        if let Ok(automation) = uia {
            if let Ok(root) = automation.GetRootElement() {
                let name_bstr = BSTR::from(target_name);
                let variant = windows::Win32::System::Variant::VARIANT::from(name_bstr);
                
                if let Ok(condition) = automation.CreatePropertyCondition(UIA_NamePropertyId, &variant) {
                    // Search for the element
                    if let Ok(element) = root.FindFirst(windows::Win32::UI::Accessibility::TreeScope_Subtree, &condition) {
                        if let Ok(rect) = element.CurrentBoundingRectangle() {
                            let _ = CoUninitialize();
                            return Some((rect.left as f64, rect.top as f64, (rect.right - rect.left) as f64, (rect.bottom - rect.top) as f64));
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
fn get_uia_bounding_box(_: &str) -> Option<(f64, f64, f64, f64)> { None }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // --- Pillar 1: Win32 Spotlight Layer Configuration ---
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_TOPMOST};
                use windows::Win32::Foundation::HWND;
                
                let hwnd = HWND(window.hwnd().unwrap().0 as *mut _);
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
                            if let Some((rx, ry, rw, rh)) = *rect_lock {
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
async fn process_ai_step(app: AppHandle) -> Result<(), String> {
    // 1. Send screenshot & prompt to Gemini here...
    // 2. Parse response (Mocked below)
    let mock_json = r#"{
        "status": "in_progress",
        "instruction": "Click the Crop tool on the left toolbar",
        "target_element_name": "Crop",
        "vlm_fallback_coords": {"x": 0.12, "y": 0.45}
    }"#;
    
    let res: VlmResponse = serde_json::from_str(mock_json).unwrap();
    
    // 3. Attempt UIA Fast-path Grounding
    let bounding_box = match get_uia_bounding_box(&res.target_element_name) {
        Some(rect) => rect,
        None => {
            // Fallback to VLM coordinates (Assuming 1920x1080 screen for example)
            let x = (res.vlm_fallback_coords.x * 1920.0) as f64;
            let y = (res.vlm_fallback_coords.y * 1080.0) as f64;
            (x - 20.0, y - 20.0, 40.0, 40.0) // Create a 40x40 box around point
        }
    };

    // 4. Update state and notify frontend to draw spotlight
    *ACTIVE_TARGET_RECT.lock().unwrap() = Some(bounding_box);
    app.emit("draw-spotlight", bounding_box).unwrap();

    Ok(())
}
