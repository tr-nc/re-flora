#!/usr/bin/env node
// One local asset origin: preview files plus the exact assets embedded by Rust.
import http from 'node:http';
import {readFile,stat} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const repo=fileURLToPath(new URL('../',import.meta.url));
const mime={'.html':'text/html','.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.json':'application/json','.glb':'model/gltf-binary','.png':'image/png'};
export function createPreviewServer(){
  return http.createServer(async(req,res)=>{
    try{
      const pathname=decodeURIComponent(new URL(req.url,'http://local').pathname);
      if(pathname==='/'){res.writeHead(302,{Location:'/model-preview/'});res.end();return;}
      const asset=pathname.startsWith('/assets/');
      const root=path.join(repo,asset?'assets':'experiments');
      const relative=asset?pathname.slice('/assets'.length):pathname;
      let file=path.resolve(root,'.'+relative);
      if(!file.startsWith(root+path.sep))throw new Error('outside served root');
      if((await stat(file)).isDirectory())file=path.join(file,'index.html');
      const bytes=await readFile(file);
      res.writeHead(200,{'Content-Type':mime[path.extname(file)]||'application/octet-stream','Cache-Control':'no-store'});res.end(bytes);
    }catch{res.writeHead(404);res.end('Not found');}
  });
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const args=process.argv.slice(2);
  if(args.length===1&&['-h','--help'].includes(args[0])){
    console.log('Usage: node scripts/serve-model-preview.mjs [--port 8765]\nServes the preview and shared game assets on 127.0.0.1 only.\nOpen http://127.0.0.1:8765/model-preview/ (use your selected port).');
  }else if(args.length!==0&&(args.length!==2||args[0]!=='--port'||!/^\d+$/.test(args[1])||+args[1]<1||+args[1]>65535)){
    console.error('Expected --port <1..65535>. Run with --help for usage.');process.exitCode=2;
  }else{
    const port=Number(args[1]||8765),server=createPreviewServer();
    server.on('error',error=>{console.error(`Preview server: ${error.message}. Choose another --port if it is in use.`);process.exitCode=1;});
    server.listen(port,'127.0.0.1',()=>console.log(`Model preview: http://127.0.0.1:${port}/model-preview/`));
  }
}
