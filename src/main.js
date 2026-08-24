import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

let currentMouseX = 0;
let currentMouseY = 0;
let targetBox = null;

const guideLine = document.getElementById('guide-line');
const hole = document.getElementById('spotlight-hole');
const border = document.getElementById('spotlight-border');
const tooltip = document.getElementById('tooltip');

// Track live global mouse coordinates
listen('mouse-move', (event) => {
    currentMouseX = event.payload.x;
    currentMouseY = event.payload.y;
    
    // Continuously redraw the magnetic bezier curve if we have a target
    if (targetBox) {
        drawBezierCurve(currentMouseX, currentMouseY, targetBox);
    }
});

// Activate the Spotlight
listen('draw-spotlight', (event) => {
    const [x, y, width, height] = event.payload;
    targetBox = { x, y, width, height };

    // Move the SVG Mask and Border
    const pad = 10;
    hole.setAttribute('x', x - pad);
    hole.setAttribute('y', y - pad);
    hole.setAttribute('width', width + (pad*2));
    hole.setAttribute('height', height + (pad*2));

    border.setAttribute('x', x - pad);
    border.setAttribute('y', y - pad);
    border.setAttribute('width', width + (pad*2));
    border.setAttribute('height', height + (pad*2));

    // Position the tooltip below the target
    tooltip.className = '';
    tooltip.innerText = "Click the target to continue";
    tooltip.style.left = `${x}px`;
    tooltip.style.top = `${y + height + 20}px`;
});

// Step Verification Success
listen('step-success', () => {
    // Hide spotlight and clear path
    targetBox = null;
    guideLine.setAttribute('d', '');
    hole.setAttribute('width', 0);
    border.setAttribute('width', 0);
    tooltip.className = 'hidden';
    
    // Play a native chime (HTML5 Audio or Tauri command)
    console.log("Step verified! Proceeding...");
});

// Draw a smooth bezier curve from (x1, y1) to the center of the target bounding box
function drawBezierCurve(mouseX, mouseY, target) {
    const targetCenterX = target.x + (target.width / 2);
    const targetCenterY = target.y + (target.height / 2);

    // Calculate control points to give it a nice "S" curve swoop
    const diffX = targetCenterX - mouseX;
    const diffY = targetCenterY - mouseY;
    
    const cp1x = mouseX + (diffX * 0.5);
    const cp1y = mouseY;
    const cp2x = targetCenterX - (diffX * 0.5);
    const cp2y = targetCenterY;

    // SVG Path data: Move to mouse, Cubic Bezier to target center
    const path = `M ${mouseX} ${mouseY} C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${targetCenterX} ${targetCenterY}`;
    guideLine.setAttribute('d', path);
}

// For testing: Trigger the mock AI response processing
setTimeout(() => invoke('process_ai_step', { userPrompt: "How do I crop this?", targetHint: "Crop Tool" }), 1000);
