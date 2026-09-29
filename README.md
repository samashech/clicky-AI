# ClickyAI

ClickyAI is a desktop visual guide under active development. **Current implementation and testing
focus: Arch Linux / Hyprland; further Windows work is on hold.** This
repository is a repaired foundation, **not a production-complete agent**.

The old fake Linux targets, mock OmniParser output, automatic demo invocation and unconditional
click success have been removed. Guidance uses Windows UI Automation or real local OCR before
asking an optional model. No Gemini call is required for exact visible-label matching.

## Run

Install Node.js, Rust and the Tauri native build prerequisites. On Windows, install Visual Studio
C++ Build Tools and WebView2. On Arch, the native dependencies include `base-devel`, `webkit2gtk-4.1`,
`openssl`, `librsvg`, `libxdo`, `libxtst`, `libxcb`, `libxrandr`, `dbus`,
`libayatana-appindicator`, `portaudio` and `espeak-ng`. Hyprland guidance additionally needs
`grim`, `gtk-layer-shell`, `python-gobject` and `python-cairo` (system Python).

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
2. Open the application you want help with. On Hyprland use tray **Activate Clicky**, or **Alt+X**
   after enabling and verifying the portal shortcut. Windows/X11 use the native shortcut.
3. A compact status indicator appears. Say “Hey Clicky AI, where is the File menu?”
4. Clicky transcribes locally, finds the control using UIA/OCR, speaks the instruction and shows
   click-through spatial guidance. Microphone capture ends before reasoning starts.
5. On Hyprland perform the instruction, then press **I did this — next step** in the popup.
   This is explicit user confirmation, not automatic click verification. Windows/X11 use geometric
   click verification. Explicit sequences such as “Click File then click Open”
   continue without another activation. Broad goals require a configured reasoning model, which
   can generate bounded click-only plans; dragging, typing and arbitrary tutorials are not implemented.
6. Completion returns to background. Alt+X again cancels; Escape cancels on Windows/X11 during an
   active session. The indicator also has a cancel button. Cancellation stops capture, speech and work.

Local speech output uses Windows SAPI or Linux eSpeak NG plus PortAudio. Missing models, devices
or dependencies produce an explicit error. Linux synthesis stays in memory and does not launch
an external audio player. Live microphone recognition still requires manual validation. Offline Vosk
recognition has been exercised with an audio fixture. The Hyprland desktop smoke test exercises
local speech output alongside a real OCR target and native overlay.

Native Wayland uses an XDG GlobalShortcuts worker. Enable the shortcut explicitly from Settings
when portal consent is required; silent startup does not open a permission dialog. The tray remains
available if the portal is unsupported. Hyprland now has a dedicated grim capture and layer-shell
guidance backend; other Wayland compositors remain unsupported. No desktop configuration is modified automatically.

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

## Test on Hyprland

Start with a visible text-labelled control, for example “Click File”. OCR cannot reliably identify
unlabelled icons. A completed step does not prove the application performed the intended action.

```sh
npm run dev
```

The native popup and spoken instruction use the same grounded task step. The highlight is
click-through; only the separate instruction panel accepts clicks. The cursor path updates locally
at about 30 Hz while guidance is active. No desktop capture, OCR or model runs periodically at idle.
Local screenshots stay in memory. `grim -s 1` keeps capture, OCR, cursor and layer-shell coordinates
in Hyprland logical pixels, avoiding double application of monitor scaling.

For a typed test without the microphone, first build with `npm run tauri build -- --debug --no-bundle`,
then run `src-tauri/target/debug/tauri-app --guide "Click File"` and focus the target application within
three seconds. Quit any already-running ClickyAI instance first. Native automated smoke test:

```sh
.venv/bin/python scripts/check-hyprland.py
```

It opens a real GTK Export button, captures and OCRs it, checks native overlay layers and speech
completion, then checks cleanup. It never records the microphone or uploads a screenshot. System
Python supplies GTK bindings; the app searches PATH for a compatible interpreter. Override with
`CLICKY_DESKTOP_PYTHON` if needed. OCR and voice use `CLICKY_PYTHON` or the development `.venv`.

## Recommended offline reasoning

Start with [Qwen3 4B](https://ollama.com/library/qwen3:4b) via Ollama (2.5 GB model download).
[Qwen3 8B](https://ollama.com/library/qwen3:8b) is a larger alternative (5.2 GB); runtime memory is
higher than download size. On this 16 GB host, 4B is the conservative first test. The NVIDIA driver
was unavailable to `nvidia-smi`, so GPU performance has not been verified. No model was downloaded
or benchmarked by the Hyprland implementation work.

Start `ollama serve` if it is not already running, then in another terminal:

```sh
ollama pull qwen3:4b
ollama run qwen3:4b
```

Warm the model before testing (requests have a 20-second timeout). In ClickyAI Settings choose
**Local**, model `qwen3:4b`, endpoint `http://127.0.0.1:11434/api/generate`, and leave cloud disabled.
The endpoint is a local process connection, not a paid/cloud API. Local requests use structured
JSON and `think: false` to avoid spending the response budget on hidden reasoning. Exact named
controls still bypass the model. Configuration is session-only; it must be reapplied after restart.

## Release blockers

Windows build/runtime and mixed-monitor hardware validation; semantic action verification;
broad application testing; AT-SPI; automatic Hyprland click/outcome verification; other Wayland compositors; broad tutorial planning;
automatic model failover; complete token accounting; persistent settings; self-contained OCR
packaging; signing; performance measurements. Do not treat compiling or passing unit tests as
completion of these gates.

See [ARCHITECTURE.md](ARCHITECTURE.md) for implementation details, the platform matrix and remaining
work, and [VALIDATION.md](VALIDATION.md) for executed checks and unverified acceptance gates. [agylearn.md](agylearn.md) records the prototype's history and is not setup documentation.

Implementation references: [Tauri shortcuts](https://v2.tauri.app/plugin/global-shortcut/),
[Hyprland IPC capabilities](https://wiki.hypr.land/Configuring/Using-hyprctl/).
