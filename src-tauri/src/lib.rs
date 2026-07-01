use tauri::{Manager, Emitter};
use device_query::{DeviceQuery, DeviceState, MouseState};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use std::thread;
use std::time::Duration;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        println!("HOTKEY ACTIVATED! Waking up AI...");
                        let _ = app.emit("hotkey-pressed", ());
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Changed hotkey to Alt+X because Krohnkite/KDE might be intercepting Ctrl+Shift+Space
            let hotkey = "alt+x".parse::<Shortcut>().unwrap();
            let _ = app.global_shortcut().register(hotkey);

            let main_window = app.get_webview_window("main").unwrap();
            
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
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
