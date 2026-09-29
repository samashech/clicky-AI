import { localPoint, curve } from './geometry.js';
const { listen } = window.__TAURI__.event;
let target=null,origin=[0,0],scale=1,pointer=null,frame=0;
const line=document.getElementById('guide-line');
const hole=document.getElementById('spotlight-hole');
const border=document.getElementById('spotlight-border');
const tooltip=document.getElementById('tooltip');
function clear(){target=null;line.setAttribute('d','');document.body.hidden=true;}
clear();
await listen('mouse-move', ({payload})=>{pointer=payload;if(target&&!frame){frame=requestAnimationFrame(()=>{frame=0;if(target)line.setAttribute('d',curve(localPoint(pointer,origin,scale),target));});}});
await listen('draw-spotlight', ({payload})=>{
    target=payload.bounds;origin=payload.origin;scale=payload.scale;document.body.hidden=false;
    for(const element of [hole,border])for(const [key,value] of Object.entries({x:target.x-7,y:target.y-7,width:target.width+14,height:target.height+14}))element.setAttribute(key,value);
    tooltip.textContent=payload.step.instruction+' · Alt+X to stop';tooltip.className='';
    tooltip.style.left=`${Math.max(12,Math.min(target.x,innerWidth-320))}px`;
    tooltip.style.top=`${Math.max(12,Math.min(target.y+target.height+20,innerHeight-90))}px`;
    line.setAttribute('d',pointer?curve(localPoint(pointer,origin,scale),target):'');
});
await listen('task-state', ({payload})=>{if(!payload||payload.status!=='waiting')clear();});
