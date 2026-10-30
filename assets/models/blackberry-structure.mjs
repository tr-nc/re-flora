// Pure, deterministic morphology. Metres are an art-scale convention, not a botanical fit.
export const defaults = Object.freeze({seed: 27, canes: 11, spread: 1, density: .78});
export const add = (a,b) => a.map((v,i)=>v+b[i]);
export const mul = (a,s) => a.map(v=>v*s);
export function random(seed) {
  let state = seed >>> 0;
  return () => { state += 0x6d2b79f5; let t=state; t=Math.imul(t^(t>>>15),t|1); t^=t+Math.imul(t^(t>>>7),t|61); return ((t^(t>>>14))>>>0)/4294967296; };
}
export function point(curve,t) {
  const u=1-t;
  return curve[0].map((_,i)=>u*u*u*curve[0][i]+3*u*u*t*curve[1][i]+3*u*t*t*curve[2][i]+t*t*t*curve[3][i]);
}
function bounded(value,fallback,min,max) {
  return Number.isFinite(Number(value)) ? Math.max(min,Math.min(max,Number(value))) : fallback;
}
export function makePlant(options={}) {
  const settings={
    seed: Math.round(bounded(options.seed,defaults.seed,0,100000)),
    canes: Math.round(bounded(options.canes,defaults.canes,5,17)),
    spread: bounded(options.spread,defaults.spread,.65,1.4),
    density: bounded(options.density,defaults.density,.2,1),
  };
  const canes=[],leaves=[],fruitSites=[];
  for(let i=0;i<settings.canes;i++) {
    const rng=random(settings.seed*131+i*997), fresh=i%3===0;
    const angle=i*2.39996+(rng()-.5)*.9;
    const direction=[Math.cos(angle),0,Math.sin(angle)];
    const root=[(rng()-.5)*.52,0,(rng()-.5)*.46];
    const reach=(fresh ? .55+rng()*.4 : 1.2+rng()*.65)*settings.spread;
    const height=(i%4===2?.68:1.05)+rng()*.65;
    const curve=[root,
      add(root,[direction[0]*.1,height*.88,direction[2]*.1]),
      add(root,[direction[0]*reach*.48,height*(fresh?1.06:1.5),direction[2]*reach*.48]),
      add(root,[direction[0]*reach,fresh?height:.2+rng()*.55,direction[2]*reach])];
    const cane={id:i,fresh,curve,radius:fresh?.010:.016,laterals:[]};
    for(let j=0;j<12;j++) {
      const t=.14+j*.074,base=point(curve,t), side=j%2?1:-1;
      const a=angle+side*(.75+rng()*.7);
      const leaf={base,angle:a,tilt:(rng()-.5)*.55,size:.23+rng()*.055,tone:Math.floor(rng()*5),rank:rng(),fresh,cane:i};
      if(leaf.rank<=settings.density) leaves.push(leaf);
      if(!fresh && j>2 && j%2===0) {
        const length=.26+rng()*.22;
        const end=add(base,[Math.cos(a)*length,.12+rng()*.16,Math.sin(a)*length]);
        const lateral={curve:[base,add(base,[Math.cos(a)*length*.3,.12,Math.sin(a)*length*.3]),add(end,[0,.03,0]),end]};
        cane.laterals.push(lateral);
        for(let k=0;k<2;k++) {
          const subLeaf={base:point(lateral.curve,.28+k*.38),angle:a+(k?-.8:.8),tilt:(rng()-.5)*.55,size:.18+rng()*.035,tone:Math.floor(rng()*5),rank:rng(),fresh:false,cane:i};
          if(subLeaf.rank<=settings.density) leaves.push(subLeaf);
        }
        if(j>=4 && rng()>.22) fruitSites.push({base:end,angle:a,cane:i,maturity:rng(),size:.85+rng()*.25});
      }
    }
    canes.push(cane);
  }
  return {settings,canes,leaves,fruitSites};
}
