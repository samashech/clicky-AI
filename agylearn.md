# ClickyAI: Rust & Tauri Crash Course

Welcome to your learning log! This file will document all the new concepts and troubleshooting steps you learn while building the ClickyAI tool. 

## 1. What is Tauri?
Tauri is a framework for building tiny, blazingly fast desktop applications. It splits your application into two parts:
* **The Frontend (`src/`):** Built using standard web technologies (HTML, CSS, JavaScript). This is what the user *sees*.
* **The Backend (`src-tauri/`):** Built using **Rust**. This handles the heavy lifting, OS integration (like tracking the mouse or capturing the screen), and interacting with the file system.

## 2. Setting Up the Rust Environment
* **Installation:** We installed Rust using `rustup`, which manages Rust toolchains (like `cargo`, the Rust package manager). 
* **Shell Gotchas:** Sourcing `.env` files written for Bash/Zsh throws errors in the **fish shell**. Fish requires its own syntax (e.g., `source ~/.cargo/env.fish`) or relying on terminal restarts for path configuration.

## 3. Arch Linux & Pacman Troubleshooting
Because Tauri needs to create native desktop windows, it requires several system-level libraries (like `webkit2gtk`, `librsvg`, etc.). 
* **Installation:** Used `sudo pacman -S --needed <packages>` to install dependencies on Arch.
* **When Pacman Gets Stuck:** If a package download hangs indefinitely, it's a mirror issue. 
  * **Fix 1:** Cancel with `Ctrl+C` and retry. Pacman remembers partial downloads and might grab a fresh connection.
  * **Fix 2:** Update your mirrorlist to find the fastest servers. On pure Arch, use `sudo reflector --latest 10 --protocol https --sort rate --save /etc/pacman.d/mirrorlist`. On Manjaro, use `sudo pacman-mirrors --fasttrack`. Always follow with `sudo pacman -Syy` to sync databases.

## 4. The Wayland Display Bug (Linux Specific)
**The Error:** `Error 71 (Protocol error) dispatching to Wayland display.`
**The Cause:** Arch Linux defaults to the Wayland display server, but Tauri's browser engine (WebKitGTK) sometimes struggles to communicate with Wayland directly, especially with certain graphics drivers.
**The Fix:** You can bypass Wayland and force the app to use X11 (via XWayland) or disable experimental WebKit renderers using environment variables:
* `GDK_BACKEND=x11 npm run tauri dev`
* `WEBKIT_DISABLE_DMABUF_RENDERER=1 npm run tauri dev`

---
## 5. Making the App an "Overlay"
To create an AI companion that floats over everything else, we need to turn the standard desktop window into an invisible overlay.
This is done in two steps:
1. **Tauri Configuration (`src-tauri/tauri.conf.json`):** We modify the window object by adding `"transparent": true` (allows the OS to render it as see-through), `"decorations": false` (removes the title bar and close buttons), and `"alwaysOnTop": true` (keeps it floating above games and browsers).
2. **CSS Styling (`src/styles.css`):** Even if the OS window is transparent, the HTML `<body>` must also be completely transparent. We remove default backgrounds and set `background-color: transparent;`.

## 6. Tracking the Mouse in Rust
To make the AI companion follow the cursor, we needed to know where the cursor is globally (even when our app isn't clicked on).
1. **The `device_query` Crate:** We used Cargo to install `device_query`, a Rust library capable of polling the OS for the current mouse and keyboard state.
2. **Background Threads:** In Rust, if we put our code in an infinite `loop {}`, the entire application would freeze! Instead, we use `std::thread::spawn()` to create a background thread.
3. **The `setup` Hook:** We hooked into Tauri's initialization phase (`.setup(|app| { ... })`) to grab a reference to the main window and pass it to our background thread. 
4. **The 60 FPS Loop:** Inside our thread, we get the `x` and `y` coordinates, set the window position (`main_window.set_position()`), and then put the thread to sleep for 16 milliseconds (`thread::sleep`). 1000ms / 60 frames = ~16ms.
## 7. The "Window Size" Illusion
When we offset the window by 20 pixels, we pinned the **top-left corner** of the invisible window to the cursor. 
If the window is `800x600` (the Tauri default), any text centered inside that window will be rendered 400 pixels to the right and 300 pixels down from the cursor!
To fix this and make our AI feel like a true companion, we:
1. Resized the invisible window to `100x100` in `tauri.conf.json`.
46: 2. Trashed the default Tauri HTML and replaced it with a tiny, glowing 40px CSS orb centered inside that 100x100 window.
47: 
48: ## 8. The GBM Buffer / Transparency Bug
49: When creating a transparent overlay on Linux (especially with NVIDIA GPUs and Wayland), WebKitGTK often fails to allocate hardware-accelerated buffers for very small transparent windows, throwing a `Failed to create GBM buffer` error.
50: **The Fixes:**
51: 1. **Increase Window Size:** We increased the invisible window size to `300x300` and updated the Rust math to subtract `130` pixels to keep the orb perfectly centered near the cursor.
52: 2. **Disable Hardware Compositing:** We ran the app with `WEBKIT_DISABLE_COMPOSITING_MODE=1` to force WebKit to stop trying to use broken hardware acceleration.
53: 3. **The Nuclear Option:** If transparency is fundamentally broken by the GPU driver, we set `"transparent": false` in `tauri.conf.json` as a temporary fallback so we can still test the logic!
54: 
55: ## 9. The Vision System (Screen Capture)
56: When the AI activates, it needs to see what the user is pointing at. We built this using the `screenshots` crate.
57: 1. When `Alt+X` is pressed, we poll `device_query` one more time to get the exact `(X, Y)` coordinates of the cursor at that exact millisecond.
58: 2. We use `screenshots::Screen::all()` to grab the main monitor and call `.capture()`.
59: 3. We save it to a temporary file (`/tmp/clickyai_vision.png`). Later, we will pass this image directly in memory to the Gemini Vision AI API along with the cursor coordinates!
