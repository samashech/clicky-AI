const {invoke}=window.__TAURI__.core;
const {listen}=window.__TAURI__.event;
function render(state){document.body.dataset.phase=state.phase;document.getElementById('phase').textContent=state.phase.replaceAll('_',' ').toLowerCase();document.getElementById('message').textContent=state.message;}
await listen('runtime-state',({payload})=>render(payload));
render(await invoke('runtime_state'));
document.getElementById('cancel').addEventListener('click',()=>invoke('cancel_task'));
