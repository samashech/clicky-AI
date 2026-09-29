import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
test('every application window is hidden at startup',async()=>{
    const config=JSON.parse(await readFile(new URL('../src-tauri/tauri.conf.json',import.meta.url),'utf8'));
    for(const window of config.app.windows)assert.equal(window.visible,false,window.label);
    const indicator=config.app.windows.find(w=>w.label==='indicator');
    assert.equal(indicator.focus,false);assert.ok(indicator.width<=360&&indicator.height<=100);
});
test('indicator renders runtime status and cancels through native command',async()=>{
    const elements=new Map(['phase','message','cancel'].map(id=>[id,{textContent:'',addEventListener(name,fn){this[name]=fn;}}]));
    const handlers=new Map(),commands=[];
    const document={body:{dataset:{}},getElementById:id=>elements.get(id)};
    const context=vm.createContext({document,window:{__TAURI__:{core:{invoke:async name=>{commands.push(name);return {phase:'IDLE',message:''};}},event:{listen:async(name,callback)=>handlers.set(name,callback)}}}});
    const source=await readFile(new URL('../src/indicator.js',import.meta.url),'utf8');
    await vm.runInContext(`(async()=>{${source}})()`,context);
    assert.equal(document.body.dataset.phase,'IDLE');
    handlers.get('runtime-state')({payload:{phase:'LISTENING',message:'Listening…'}});
    assert.equal(elements.get('message').textContent,'Listening…');
    elements.get('cancel').click();assert.equal(commands.at(-1),'cancel_task');
    handlers.get('runtime-state')({payload:{phase:'IDLE',message:''}});
    assert.equal(document.body.dataset.phase,'IDLE');
});
