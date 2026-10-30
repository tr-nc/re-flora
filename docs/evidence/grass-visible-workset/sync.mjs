import fs from 'node:fs';import path from 'node:path';import {spawnSync} from 'node:child_process';import {createHash} from 'node:crypto';
const root=process.cwd(),out=path.resolve('target/grass-workset/sync');fs.mkdirSync(out,{recursive:true});const runs=[];
for(const name of ['step1','final']) {
 const runtime=path.join(out,'runtime');fs.rmSync(runtime,{recursive:true,force:true});fs.mkdirSync(runtime,{recursive:true});fs.cpSync('config',path.join(runtime,'config'),{recursive:true});fs.symlinkSync(path.join(root,'assets'),path.join(runtime,'assets'));
 const binary=path.join(root,'target/grass-workset',name),env={...process.env};for(const key of Object.keys(env))if(key.startsWith('RE_FLORA_'))delete env[key];delete env.WAYLAND_DISPLAY;
 Object.assign(env,{RUST_LOG:'info',RE_FLORA_ROOT:runtime,RE_FLORA_GRASS_VISIBILITY_AUDIT:'1',RE_FLORA_STEM_PIXEL_PROFILE:'1',VK_LAYER_PATH:'/home/terence/vulkan-sdk/x86_64/share/vulkan/explicit_layer.d',VK_INSTANCE_LAYERS:'VK_LAYER_KHRONOS_validation',VK_LAYER_VALIDATE_SYNC:'1',LD_LIBRARY_PATH:'/home/terence/vulkan-sdk/x86_64/lib:'+(env.LD_LIBRARY_PATH??'')});
 const args=['--hidden','--mute','--perf','--camera-snapshot','player-default','--auto-exit','2'];console.log(name);
 const p=spawnSync(binary,args,{env,encoding:'utf8',maxBuffer:256*1024*1024}),log=(p.stdout??'')+(p.stderr??'');fs.writeFileSync(path.join(out,name+'.log'),log);
 runs.push({name,args,status:p.status,binaryHash:createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),errors:log.split('\n').filter(s=>/\bERROR\b|VUID-|SYNC-HAZARD|panicked at/.test(s)),complete:/phase=complete failures=0/.test(log),lastAudit:[...log.matchAll(/GRASS_VISIBILITY_VALIDATED\] (.*)/g)].at(-1)?.[1]});
 fs.writeFileSync(path.join(out,'summary.json'),JSON.stringify(runs,null,2));console.log(JSON.stringify(runs.at(-1)));
}fs.rmSync(path.join(out,'runtime'),{recursive:true,force:true});
