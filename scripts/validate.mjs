import {spawnSync} from 'node:child_process';
const commands=[['node',['--test','tests/geometry.test.mjs','tests/overlay.test.mjs','tests/background.test.mjs']], [process.platform==='win32'?'python':'python3',['-m','unittest','discover','-s','tests','-p','test_*.py']],['cargo',['test','--manifest-path','src-tauri/Cargo.toml','--locked']]];
for(const [bin,args] of commands){const result=spawnSync(bin,args,{stdio:'inherit'});if(result.status!==0)process.exit(result.status||1);}
