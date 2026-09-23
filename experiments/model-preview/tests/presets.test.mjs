import test from 'node:test';
import assert from 'node:assert/strict';
import {validatePreset} from '../presets.mjs';
const definitions=[{id:'example',controls:[{key:'curl',label:'curl',min:-1,max:1},{key:'color',label:'color',type:'color'},{key:'shadows',label:'shadows',type:'checkbox'}]}];
const preset=()=>({version:1,model:'example',modelSettings:{curl:.5,color:'#FF8800',shadows:true},layout:'compare',wireframe:false,
  processing:{resolution:32,repair:true,levels:0},animation:{clip:0,time:.237,fps:60,speed:1},
  view:{projection:'orthographic',position:[0,0,6],target:[0,0,0],zoom:1},appearance:{background:'#253039',checker:false}});

test('versioned presets contain validated model and shared state',()=>{
  const result=validatePreset(preset(),definitions);
  assert.equal(result.modelSettings.color,'#ff8800');assert.equal(result.animation.time,.237);
  assert.deepEqual(validatePreset(JSON.parse(JSON.stringify(result)),definitions),result);
});
test('bad versions, model IDs, ranges and non-finite data are rejected before loading',()=>{
  for(const mutate of [p=>p.version=2,p=>p.model='other',p=>p.processing.resolution=0,p=>p.processing.resolution=8.5,
    p=>p.modelSettings.curl=NaN,p=>p.modelSettings.shadows='true',p=>p.modelSettings.color='red',
    p=>p.view.position=[0,0,0],p=>p.view.zoom=Infinity,p=>p.animation.fps=61,p=>p.appearance.background='url(x)']){
    const p=preset();mutate(p);assert.throws(()=>validatePreset(p,definitions),/预设无效/);
  }
});
