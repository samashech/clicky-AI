const { listen } = window.__TAURI__.event;

// When the page loads, we set up our event listener
window.addEventListener("DOMContentLoaded", () => {
  const aiCompanion = document.getElementById("ai-companion");

  // Listen for the "hotkey-pressed" event sent from our Rust backend
  listen("hotkey-pressed", (event) => {
    console.log("Heard the hotkey from Rust!");
    
    // Add the 'listening' class to change the color of the orb
    aiCompanion.classList.add("listening");

    // Remove the class after 2 seconds to simulate "finishing listening"
    // (Later, we will remove this when the AI actually finishes talking)
    setTimeout(() => {
      aiCompanion.classList.remove("listening");
    }, 2000);
  });
});
