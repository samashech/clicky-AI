let mediaRecorder = null;
let audioChunks = [];

// Request microphone access on startup
window.addEventListener("DOMContentLoaded", async () => {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    mediaRecorder = new MediaRecorder(stream, { mimeType: 'audio/webm' });

    mediaRecorder.ondataavailable = (event) => {
      if (event.data.size > 0) {
        audioChunks.push(event.data);
      }
    };

    mediaRecorder.onstop = async () => {
      const audioBlob = new Blob(audioChunks, { type: 'audio/webm' });
      audioChunks = []; // Reset for next recording
      
      // Convert Blob to Base64 to send to Rust
      const reader = new FileReader();
      reader.readAsDataURL(audioBlob);
      reader.onloadend = async () => {
        const base64Audio = reader.result.split(',')[1];
        
        // Grab the coordinates that were passed into stopRecording
        const { x, y } = window.__cursorCoords;
        
        console.log("Sending audio to Rust...");
        // Call our new Rust command
        const { invoke } = window.__TAURI__.core;
        await invoke('process_audio', { audioBase64: base64Audio, x: x, y: y });
      };
    };
  } catch (err) {
    console.error("Microphone access denied or not found:", err);
  }
});

// These functions will be executed directly from Rust via app.eval()
window.startRecording = () => {
  const orb = document.getElementById('ai-companion');
  orb.classList.add('listening');
  if (mediaRecorder && mediaRecorder.state === 'inactive') {
    mediaRecorder.start();
    console.log("Microphone recording started...");
  }
};

window.stopRecording = (x, y) => {
  const orb = document.getElementById('ai-companion');
  orb.classList.remove('listening');
  
  // Store the coordinates globally so the onstop event can access them
  window.__cursorCoords = { x, y };

  if (mediaRecorder && mediaRecorder.state === 'recording') {
    mediaRecorder.stop();
    console.log("Microphone recording stopped.");
  }
};
