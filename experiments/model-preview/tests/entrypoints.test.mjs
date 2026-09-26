import test from 'node:test';
import assert from 'node:assert/strict';
import {readdir,access} from 'node:fs/promises';

test('preview has one unnumbered entry instead of numbered comparison pages',async()=>{
  const files=await readdir(new URL('../../butterfly-method-comparison/',import.meta.url));
  assert.deepEqual(files.filter(name=>/^comparison-v\d+\.html$/.test(name)),[]);
  await access(new URL('../index.html',import.meta.url));
  await access(new URL('../../../assets/models/butterfly.glb',import.meta.url));
});
