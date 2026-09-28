// Shared selected topology: one stalk and its terminal complete head.
// The stalk always uses the single-column voxel surface; the calyx stays in
// the complete head. The browser and native publisher consume this same recipe.
import {stemColumn,voxelStemSurface} from './flower-stem.mjs';

function leafComponents(part){
  const parent=Array.from({length:part.positions.length/3},(_,i)=>i);
  const find=i=>{while(parent[i]!==i){parent[i]=parent[parent[i]];i=parent[i];}return i;};
  for(let i=0;i<part.indices.length;i+=3){
    const [a,b,c]=part.indices.slice(i,i+3);parent[find(b)]=find(a);parent[find(c)]=find(a);
  }
  const groups=new Map();
  for(const i of [...new Set(part.indices)].sort((a,b)=>a-b)){
    const root=find(i);if(!groups.has(root))groups.set(root,[]);groups.get(root).push(i);
  }
  return [...groups.values()];
}
export function singleStemFlower(authored,settings,leafRootVertex=0){
  const originalHead=authored.heads[0];
  if(!originalHead)throw new Error('Single-stem flower requires a terminal head');
  const column=stemColumn(originalHead.anchor[1],settings.bend??0);
  const stemShape=voxelStemSurface(column);
  const parts=[{name:'Single stem',material:'stemColor',head:null,...stemShape}],leafAttachments=[];
  for(const source of authored.parts.filter(part=>part.head===null&&part.material==='leafColor')){
    const positions=source.positions.slice();
    for(const component of leafComponents(source)){
      // Legacy blade recipes start each connected leaflet with its root vertex;
      // fan recipes explicitly declare the root offset at the preview boundary.
      const rootIndex=component[0]+leafRootVertex;
      if(!component.includes(rootIndex))throw new Error('Missing authored leaf attachment');
      const sourceRoot=source.positions.slice(rootIndex*3,rootIndex*3+3);
      const t=Math.max(0,Math.min(1,(sourceRoot[1]-column.root[1])/(originalHead.anchor[1]-column.root[1])));
      const layer=Math.max(1,Math.min(column.cells.length-2,Math.round(t*(column.cells.length-1))));
      const root=column.cells[layer].center;
      for(const i of component)for(let k=0;k<3;k++)positions[i*3+k]+=root[k]-sourceRoot[k];
      leafAttachments.push({root:[...root],sourceRoot,vertices:component,rootIndex});
    }
    parts.push({...source,positions,indices:source.indices.slice()});
  }
  for(const source of authored.parts.filter(part=>part.head===originalHead.id)){
    parts.push({...source,head:0,positions:source.positions.map((n,i)=>n+column.tip[i%3]-originalHead.anchor[i%3]),indices:source.indices.slice()});
  }
  return {parts,heads:[{id:0,anchor:[...column.tip],label:'顶端花头'}],column,leafAttachments};
}
