import test from 'node:test';
import assert from 'node:assert/strict';
import {readdir,access} from 'node:fs/promises';

test('preview has one unnumbered entry instead of numbered comparison pages',async()=>{
  const files=await readdir(new URL('../../butterfly-method-comparison/',import.meta.url));
  assert.deepEqual(files.filter(name=>/^comparison-v\d+\.html$/.test(name)),[]);
  await access(new URL('../index.html',import.meta.url));
  // Source asset paths are intentionally unchanged by the viewer cleanup.
  await access(new URL('../../butterfly-method-comparison/blender-v5/butterfly-prototype.glb',import.meta.url));
});
