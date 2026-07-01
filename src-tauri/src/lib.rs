use tauri::{Manager, Emitter};
use device_query::{DeviceQuery, DeviceState, MouseState};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use std::thread;
use std::time::Duration;

#[tauri::command]
async fn process_audio(audio_base64: String, x: i32, y: i32) -> Result<(), String> {
    println!("Received audio payload ({} bytes)! Mouse was at ({}, {})", audio_base64.len(), x, y);
    // In the next step, we will send this audio to OpenAI Whisper!
    Ok(())
}

use std::sync::{Arc, Mutex};
use std::process::{Command, Child};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let recording_process = Arc::new(Mutex::new(None::<Child>));
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        let mut rec = recording_process.lock().unwrap();
                        
                        if rec.is_none() {
                            // START RECORDING
                            println!("HOTKEY TOGGLED! Starting microphone via native OS...");
                            let js_code = "document.getElementById('ai-companion').classList.add('listening');";
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.eval(js_code);
                            }
                            
                            // Spawn native Linux arecord process to bypass ALL browser permissions!
                            if let Ok(child) = Command::new("arecord")
                                .args(["-f", "S16_LE", "-c", "1", "-r", "16000", "/tmp/clickyai_audio.wav"])
                                .spawn() {
                                    *rec = Some(child);
                            }
                        } else {
                            // STOP RECORDING
                            println!("HOTKEY TOGGLED! Stopping microphone & capturing screen...");
                            
                            let js_code = "document.getElementById('ai-companion').classList.remove('listening');";
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.eval(js_code);
                            }
                            
                            // Kill the native recording process
                            if let Some(mut child) = rec.take() {
                                let _ = child.kill();
                                let _ = child.wait();
                                if let Ok(audio_bytes) = std::fs::read("/tmp/clickyai_audio.wav") {
                                    println!("Successfully recorded {} bytes of audio natively!", audio_bytes.len());
                                }
                            }
                            
                            let device_state = DeviceState::new();
                            let mouse = device_state.get_mouse();
                            let (cursor_x, cursor_y) = mouse.coords;
                            
                            if let Ok(screens) = screenshots::Screen::all() {
                                if let Some(screen) = screens.first() {
                                    if let Ok(image) = screen.capture() {
                                        let path = "/tmp/clickyai_vision.png";
                                        let _ = image.save(path);
                                        println!("Vision captured! Cursor at: ({}, {})", cursor_x, cursor_y);
                                    }
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Changed hotkey to Alt+X because Krohnkite/KDE might be intercepting Ctrl+Shift+Space
            let hotkey = "alt+x".parse::<Shortcut>().unwrap();
            let _ = app.global_shortcut().register(hotkey);

            let main_window = app.get_webview_window("main").unwrap();
            
            // This is the magic line that allows you to click THROUGH the overlay!
            let _ = main_window.set_ignore_cursor_events(true);
            
            // Spawn a background thread to continuously track the mouse
            thread::spawn(move || {
                let device_state = DeviceState::new();
                
                loop {
                    // 1. Mouse Tracking
                    let mouse: MouseState = device_state.get_mouse();
                    let (x, y) = mouse.coords;
                    
                    // We shrunk the window back down to 50x50. 
                    // So we subtract 5 to keep the 40x40 orb centered on the cursor!
                    let _ = main_window.set_position(
                        tauri::Position::Physical(tauri::PhysicalPosition::new(x - 5, y - 5))
                    );
                    
                    // Sleep for 16 milliseconds to run at roughly 60 Frames Per Second
                    thread::sleep(Duration::from_millis(16));
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![process_audio])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
