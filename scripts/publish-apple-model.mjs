#!/usr/bin/env node
// Deterministic render mesh derived from the same geometry the HTML preview draws.
import {readFile,writeFile} from 'node:fs/promises';
import {crc32} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {appleGeometry} from '../assets/models/apple-source.mjs';

export async function publishedApple(){
  const source=await readFile(new URL('../assets/models/apple-source.mjs',import.meta.url));
  const publisher=await readFile(new URL('./publish-apple-model.mjs',import.meta.url));
  return JSON.stringify({source_crc32:crc32(Buffer.concat([source,publisher])),parts:appleGeometry()})+'\n';
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length!==2){
    console[process.argv[2]==='--help'?'log':'error']('Usage: node scripts/publish-apple-model.mjs\nRebuilds assets/models/apple-preview.json from apple-source.mjs.');
    if(process.argv[2]!=='--help')process.exitCode=2;
  }else{
    await writeFile(new URL('../assets/models/apple-preview.json',import.meta.url),await publishedApple());
    console.log('Published assets/models/apple-preview.json');
  }
}
