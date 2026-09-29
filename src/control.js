const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = id => document.getElementById(id);
const status = message => { $('status').textContent = message; };
$('task-form').addEventListener('submit', async event => {
    event.preventDefault();
    try { await invoke('start_task', { goal: $('goal').value, mode: $('mode').value }); }
    catch (error) { status(String(error)); }
});
$('cancel').addEventListener('click', () => invoke('cancel_task').catch(error => status(String(error))));
$('settings-form').addEventListener('submit', async event => {
    event.preventDefault();
    try {
        await invoke('configure', { hotkey: $('hotkey').value, config: {kind:$('provider').value,endpoint:$('endpoint').value,model:$('model').value,cloud_enabled:$('cloud').checked} });
        status('Preferences applied for this session.');
        await diagnostics();
    } catch (error) { status(String(error)); }
});
await listen('task-state', ({ payload: task }) => {
    if (!task) return;
    const phase = task.status === 'complete' ? 'All clicks verified.' : `Step ${Math.min(task.current + 1, task.steps.length)} of ${task.steps.length} · ${task.status}`;
    status(`${phase} ${task.message || ''}`);
    $('steps').replaceChildren(...task.steps.map(step => {const li=document.createElement('li');li.textContent=`${step.status === 'complete' ? '✓ ' : ''}${step.instruction}${task.mode === 'explain' && step.target ? ` — located by ${step.target.source}, ${Math.round(step.target.confidence * 100)}% confidence` : ''}`;return li;}));
});
async function diagnostics() {
    const result=await invoke('diagnostics');
    $('capabilities').textContent=JSON.stringify(result, null, 2);
    if(result.session==='wayland') {
        $('hotkey').disabled=true;
        status('Wayland detected. Voice activation is available through the tray; native screen guidance is not yet supported.');
        document.querySelector('footer').textContent='Wayland shortcuts are desktop-managed. Use the tray if unavailable.';
    }
}
await diagnostics().catch(error => status(String(error)));
await listen('settings-section',({payload})=>{if(payload==='diagnostics'){$('diagnostics').open=true;$('diagnostics').scrollIntoView();}diagnostics();});
const voiceConfig=await invoke('voice_settings');
$('stt-provider').value=voiceConfig.recognition.provider;
$('model-path').value=voiceConfig.recognition.model_path;
$('microphone').value=voiceConfig.recognition.microphone??'';
$('stt-endpoint').value=voiceConfig.recognition.endpoint;
$('stt-model').value=voiceConfig.recognition.model;
$('tts-provider').value=voiceConfig.synthesis.provider;
$('tts-voice').value=voiceConfig.synthesis.voice;
$('tts-endpoint').value=voiceConfig.synthesis.endpoint;
$('tts-model').value=voiceConfig.synthesis.model;
$('cloud-audio').checked=voiceConfig.recognition.cloud_audio||voiceConfig.synthesis.cloud_audio;
$('voice-form').addEventListener('submit',async event=>{
    event.preventDefault();
    const recognition={provider:$('stt-provider').value,model_path:$('model-path').value,endpoint:$('stt-endpoint').value,model:$('stt-model').value,cloud_audio:$('cloud-audio').checked,microphone:$('microphone').value===''?null:Number($('microphone').value),voice:''};
    const synthesis={provider:$('tts-provider').value,model_path:'',endpoint:$('tts-endpoint').value,model:$('tts-model').value,cloud_audio:$('cloud-audio').checked,microphone:null,voice:$('tts-voice').value};
    try{await invoke('configure_voice',{config:{recognition,synthesis}});status('Voice settings applied. Microphone remains off until activation.');}catch(error){status(String(error));}
});
$('refresh-diagnostics').addEventListener('click',async()=>{try{$('voice-capabilities').textContent=JSON.stringify(await invoke('voice_diagnostics'),null,2);}catch(error){$('voice-capabilities').textContent=String(error);}});

$('portal-hotkey').addEventListener('click',async()=>{await invoke('enable_wayland_hotkey');status('Shortcut request sent to the desktop. Approve its prompt if shown, then refresh Diagnostics.');});
