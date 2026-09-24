import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {publishedLeaf} from '../../../scripts/publish-leaf-model.mjs';
import {leafGeometry,leafDefaults} from '../../../assets/models/leaf-source.mjs';
import {createPreviewServer} from '../../../scripts/serve-model-preview.mjs';

test('published leaf is exactly regenerated from the one authoring recipe',async()=>{
  assert.deepEqual(await readFile(new URL('../../../assets/models/leaf.glb',import.meta.url)),await publishedLeaf());
  const data=leafGeometry();assert.equal(data.positions.length/3,29);assert.equal(data.indices.length/3,32);
  const curled=leafGeometry({...leafDefaults,curl:.8});assert.notDeepEqual(data.positions,curled.positions);assert.deepEqual(data.indices,curled.indices);
});
test('widest-point control moves the broadest row while preserving maximum width and tapered ends',()=>{
  const rows=position=>{
    const vertices=leafGeometry({...leafDefaults,widestPoint:position}).positions;
    return Array.from({length:7},(_,i)=>{
      const left=(1+i*3)*3,right=(3+i*3)*3;
      return vertices[right]-vertices[left];
    });
  };
  const base=rows(.5),nearStem=rows(0),nearTip=rows(1);
  assert.deepEqual([nearStem.indexOf(Math.max(...nearStem)),base.indexOf(Math.max(...base)),nearTip.indexOf(Math.max(...nearTip))],[0,3,6]);
  for(const widths of [nearStem,nearTip])assert.ok(Math.abs(Math.max(...widths)-Math.max(...base))<1e-6);
  assert.equal(leafGeometry({...leafDefaults,widestPoint:0}).positions.length,leafGeometry().positions.length);
});
test('preview serves the exact canonical game bytes, not experiment copies',async()=>{
  const server=createPreviewServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  try{
    for(const model of ['leaf','butterfly']){
      const response=await fetch(`http://127.0.0.1:${server.address().port}/assets/models/${model}.glb`);
      assert.equal(response.status,200);assert.deepEqual(Buffer.from(await response.arrayBuffer()),await readFile(new URL(`../../../assets/models/${model}.glb`,import.meta.url)));
    }
  }finally{await new Promise(resolve=>server.close(resolve));}
});
