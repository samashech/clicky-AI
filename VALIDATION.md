# Validation record — 2026-09-27

## Executed on Arch Linux / Hyprland

- `npm install`: passed after permitting dependency downloads.
- `cargo check --manifest-path src-tauri/Cargo.toml --locked`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml`: 18 tests passed. The HTTP integration
  test requires loopback binding; the restricted sandbox denied that operation, then the
  same test passed with the required access. No test was skipped to hide the failure.
- `npm test`: three test files passed (coordinate transforms, curve endpoint, overlay event
  handling, cancellation and scaled pointer rendering).
- `python3 -m unittest discover -s tests -p 'test_*.py'`: ten tests passed, including real
  Tesseract OCR on a generated text image. No mock detector substitutes for that test.
- `npm run tauri build -- --debug --no-bundle`: passed.
- `npm run dev` (Tauri dev): compiled and launched without the earlier GTK abort.
- `npm run tauri build`: Linux release executable, deb and rpm generated.
- Inspected deb contents: worker installed as `usr/lib/ClickyAI/perception/main.py`.
- Native launch: initial hidden-window click-through call caused a GTK/Tao abort. Corrected
  initialization; subsequent launch remained running. Subsequent background conversion was verified with no mapped ClickyAI windows, a registered tray,
  explicit Settings opening, and close-to-background via `scripts/check-background.py`.
- Windows UIA function: compiled independently against windows-rs 0.52 bindings on Linux.
  This is syntax/API validation only, **not a Windows target build or runtime test**.
- `git diff --check`: passed.

The screenshots crate reports a future-Rust compatibility warning. It is not a current build
error; replacing/updating capture must preserve the physical-pixel coordinate contract.

## Background and voice checks

- All configured windows start hidden; frontend tests cover compact indicator state/cancellation.
- Rust tests cover legal runtime transitions, same-target speech/overlay instructions, plan validation,
  cloud-audio consent and actual subprocess cancellation (heartbeat stops when the job is dropped).
- Native Hyprland smoke: hidden startup, all tray entries, Settings opens, Settings closes back to
  background. `scripts/check-background.py --quit` also verified tray removal; the dev process
  exited successfully and no `tauri-app` process remained. No microphone capture was activated.
- Rebuilt Linux deb/rpm with the background implementation; inspected deb contents and confirmed
  all three voice worker modules and the perception worker are included.
- Offline Vosk small English model recognized the official example audio locally.
- eSpeak NG synthesized non-silent 22,050 Hz PCM in memory using temporary extracted Arch libraries.
  This proves synthesis, not live audio playback; system eSpeak installation is still required.

## Not validated / not implemented

- Live microphone capture, audible playback, native Alt+X/Escape and portal consent/activation have
  not been tested on desktop hardware. No complete voice-to-verified-target demo is claimed.
- No Windows executable or installer was built or launched here. Windows CI is configured,
  but has not been run remotely by this session.
- No real Windows/X11 application was driven through the entire guidance sequence.
- No actual mixed-DPI or multi-monitor Windows hardware test was performed. Unit tests prove
  the transform math, not the native platform integration.
- Geometric hits are tested; semantic application outcomes are not implemented.
- Native Wayland guidance is explicitly disabled, with an explanatory error. The successful
  control-window launch does not imply capture, input observation or overlay support.
- Real cloud model credentials/endpoints were not exercised. Network tests use a local HTTP
  server; schema, privacy routing and retry/dedup behavior are covered independently.
- Broad tutorial actions, app adapters, AT-SPI, provider fallback chains, perceptual
  image hashing, comprehensive telemetry and persistent settings remain unimplemented.
- Installers contain the Python script but do not install Python/Tesseract dependencies.
- No release signing, installer installation/uninstallation test, Arch PKGBUILD, memory/idle
  benchmark or cross-application compatibility matrix has been completed.

## Next manual gate

On Windows, run the CI-equivalent build, install OCR dependencies, open a menu-based application,
and try `Click File then click Open`. Check that outside clicks never advance, each new step
reacquires its target, Alt+X cancels promptly, and closing Settings keeps the background process alive; tray Quit exits. Repeat with
100%, 125%, 150%, 175%, 200% scaling and monitors arranged with negative desktop coordinates.
Record actual application/version/display results before upgrading any capability to verified.
