// Authoring output only: one complete flower in attachment-local coordinates.
// Stem construction, placement and motion belong to the game, not this module.
export function completeFlowerHead(authored){
  const head=authored.heads[0];
  if(!head)throw new Error('Flower model requires a complete head');
  const parts=authored.parts.filter(part=>part.head===head.id).map(part=>({
    ...part,head:0,positions:part.positions.map((value,i)=>value-head.anchor[i%3]),indices:part.indices.slice(),
  }));
  return {parts,heads:[{id:0,anchor:[0,0,0],label:'完整花头'}],socketNormal:authored.socketNormal};
}
