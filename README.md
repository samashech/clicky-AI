# ClickyAI

ClickyAI is a blazingly fast desktop AI companion built with Rust and Tauri. 

It functions as an invisible overlay that tracks your mouse cursor globally across your screen. With a simple hotkey (`Alt+X`), ClickyAI captures the screen around your cursor to "see" what you are pointing at and provides intelligent context using Vision AI.

## Features

- **Global Mouse Tracking**: Follows your cursor seamlessly across all applicati
- **Invisible Overlay**: Runs in a transparent, frameless window to feel like a true desktop companion.
- **Screen Capture Vision**: Captures the screen at your exact cursor location when triggered.

## Technologies Used

- **Frontend**: Vanilla HTML, CSS, JavaScript.
- **Backend**: Rust and Tauri (for OS integration, global mouse tracking, and screen capture).

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/)

### Installation

1. Install dependencies:
   ```bash
   npm install
   ```
2. Run the application in development mode:
   ```bash
   npm run tauri dev
   ```

*(Note for Linux users: You may need to run `WEBKIT_DISABLE_COMPOSITING_MODE=1 npm run tauri dev` or `GDK_BACKEND=x11 npm run tauri dev` if you experience transparency or Wayland issues.)*

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
