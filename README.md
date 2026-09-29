# ClickyAI

ClickyAI is a desktop visual guide under active development, targeting **Windows 10/11 first**
and **Arch-based Linux second**. It highlights real controls and verifies pointer hits. This
repository is a repaired foundation, **not a production-complete agent**.

The old fake Linux targets, mock OmniParser output, automatic demo invocation and unconditional
click success have been removed. Guidance uses Windows UI Automation or real local OCR before
asking an optional model. No Gemini call is required for exact visible-label matching.

## Run

Install Node.js, Rust and the Tauri native build prerequisites. On Windows, install Visual Studio
C++ Build Tools and WebView2. On Arch, the native dependencies include `base-devel`, `webkit2gtk-4.1`,
`openssl`, `librsvg`, `libxdo`, `libxtst`, `libxcb`, `libxrandr`, `dbus`,
`libayatana-appindicator`, `portaudio` and `espeak-ng`.

For OCR, install Python, the Tesseract executable plus English language data, then:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r omni_server/requirements.txt -r voice/requirements.txt
export CLICKY_PYTHON="$PWD/.venv/bin/python"
npm install
npm run dev
```

On Windows PowerShell, use `python -m venv .venv`, `.venv\Scripts\python.exe -m pip install -r
omni_server/requirements.txt -r voice/requirements.txt`, and `$env:CLICKY_PYTHON = "$PWD\.venv\Scripts\python.exe"`.
Tesseract must be on PATH. Without OCR dependencies, Windows UIA can still ground matching native
controls; OCR failures are explicit. Model inference is off by default.

```sh
npm run validate                       # frontend, real OCR and Rust tests
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build                    # platform bundle
npm run tauri build -- --debug --no-bundle
```

`cargo check` / `cargo test` also work from `src-tauri/`. Python tests require the real OCR
dependencies, including Tesseract. CI builds Windows NSIS and Linux deb/rpm artifacts. These
installers bundle Python worker source but require Python, OCR/voice dependencies and an offline speech
model separately. Set `CLICKY_PYTHON` and `CLICKY_STT_MODEL` for installed releases; they are not standalone.

## Background voice flow

`npm run dev` launches the native Tauri app with every window hidden. The tray provides Activate,
Pause / Resume, Settings, Diagnostics and Quit. Closing Settings hides it; Quit exits the app.
There is no startup dashboard. Settings and the optional typed-request fallback open only on request.

1. Install an offline [Vosk model](https://alphacephei.com/vosk/models). Extract the small English
   model to `.models/vosk-model-small-en-us-0.15`, or set `CLICKY_STT_MODEL` to its extracted directory.
   Development mode automatically finds that model and the repository `.venv`.
2. On Windows or X11, open the application you want help with and press **Alt+X**.
3. A compact status indicator appears. Say “Hey Clicky AI, where is the File menu?”
4. Clicky transcribes locally, finds the control using UIA/OCR, speaks the instruction and shows
   click-through spatial guidance. Microphone capture ends before reasoning starts.
5. Click inside the target to advance. Explicit sequences such as “Click File then click Open”
   continue without another activation. Broad goals require a configured reasoning model, which
   can generate bounded click-only plans; dragging, typing and arbitrary tutorials are not implemented.
6. Completion returns to background. Alt+X again cancels; Escape cancels on Windows/X11 during an
   active session. The indicator also has a cancel button. Cancellation stops capture, speech and work.

Local speech output uses Windows SAPI or Linux eSpeak NG plus PortAudio. Missing models, devices
or dependencies produce an explicit error. Linux synthesis stays in memory and does not launch
an external audio player. Live microphone recognition and audible playback still require hardware
validation. Offline Vosk recognition and eSpeak synthesis have been exercised with fixtures.

Native Wayland uses an XDG GlobalShortcuts worker. Enable the shortcut explicitly from Settings
when portal consent is required; silent startup does not open a permission dialog. The tray remains
available if the portal is unsupported. **Wayland screen guidance remains unavailable** until native
capture/input/overlay integration is completed. No desktop configuration is modified automatically.

## Optional models and privacy

Preferences accept a complete endpoint and model for local Ollama-compatible, OpenAI-compatible,
or Gemini reasoning. Local endpoints must be loopback. Cloud requires HTTPS, an explicit checkbox
and `CLICKY_API_KEY` in the app's environment. The request endpoint is configurable; Gemini uses
a complete model-specific generateContent endpoint. Preferences last for the session.

Screenshots **never go to cloud**. When cloud is enabled, compact visible UI labels can be sent,
which may still contain private information. No task history or screenshot files are saved.
Requests, output sizes and retries are bounded. Exact-label matching needs no LLM. No live cloud
provider validation has been performed in this environment.

Speech settings are separate from reasoning settings. Optional cloud audio requires explicit
consent, a complete HTTPS endpoint and `CLICKY_SPEECH_API_KEY`. Local recognition is the default;
no microphone is opened while idle. Preferences currently last only for the running process.

## Linux and Wayland status

Hidden startup, tray management and Settings close-to-background have been tested on Arch/Hyprland. **Native Wayland guidance is disabled**:
portal capture, pointer observation, AT-SPI and dependable compositor overlay placement are not
integrated. The shortcut portal client is implemented but native key activation is not yet validated. Diagnostics expose this rather than returning fake targets. XWayland is not treated
as proof of native Wayland support. X11 guidance is implemented but still needs desktop validation.

## Release blockers

Windows build/runtime and mixed-monitor hardware validation; semantic action verification;
real application end-to-end testing; AT-SPI; Wayland capture/input; broad tutorial planning;
automatic model failover; complete token accounting; persistent settings; self-contained OCR
packaging; signing; performance measurements. Do not treat compiling or passing unit tests as
completion of these gates.

See [ARCHITECTURE.md](ARCHITECTURE.md) for implementation details, the platform matrix and remaining
work, and [VALIDATION.md](VALIDATION.md) for executed checks and unverified acceptance gates. [agylearn.md](agylearn.md) records the prototype's history and is not setup documentation.

Implementation references: [Tauri shortcuts](https://v2.tauri.app/plugin/global-shortcut/),
[Hyprland IPC capabilities](https://wiki.hypr.land/Configuring/Using-hyprctl/).
