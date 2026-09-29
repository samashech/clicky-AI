import test from 'node:test';
import assert from 'node:assert/strict';
import {localPoint,curve} from '../src/geometry.js';
test('physical pointer maps into negative-origin monitor at all required scales',()=>{for(const scale of [1,1.25,1.5,1.75,2])assert.deepEqual(localPoint({x:-1800,y:100},[-1920,0],scale),{x:120/scale,y:100/scale});});
test('magnetic path ends at the actual target center',()=>{assert.match(curve({x:0,y:0},{x:100,y:200,width:60,height:40}),/130 220$/);});
