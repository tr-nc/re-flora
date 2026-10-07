// Original preview packaging marks, authored on a coarse grid for small pixel tiles.
// One large silhouette + one solid title block; no lettering, logos or brand assets.
export const soilBagStickerGrid=Object.freeze({width:32,height:40});
const inside=(x,y,points)=>{
  let hit=false;
  for(let i=0,j=points.length-1;i<points.length;j=i++){
    const [a,b]=points[i],[c,d]=points[j];
    if((b>y)!==(d>y)&&x<(c-a)*(y-b)/(d-b)+a)hit=!hit;
  }
  return hit;
};
const leaves=[[[4,8],[10,8],[15,12],[17,19],[10,18],[6,14]],[[28,6],[21,6],[16,11],[15,19],[22,16],[26,12]]];
const layers=Array.from({length:3},(_,i)=>[[4,11],[12,8],[20,10],[28,9],[28,14],[20,15],[12,13],[4,16]].map(([x,y])=>[x,y+i*7]));

export function soilBagStickerMask(design='sprout'){
  const {width,height}=soilBagStickerGrid,mask=new Uint8Array(width*height);
  for(let y=0;y<height;y++)for(let x=0;x<width;x++){
    const px=x+.5,py=y+.5;
    let ink;
    if(design==='layers')ink=layers.some(points=>inside(px,py,points));
    else if(design==='flower'){
      ink=false;
      for(let petal=0;petal<5;petal++){
        const angle=-Math.PI/2+petal*Math.PI*2/5,c=Math.cos(angle),s=Math.sin(angle);
        const dx=px-16-7*c,dy=py-18-7*s;
        if(((dx*c+dy*s)/6)**2+((-dx*s+dy*c)/4.8)**2<=1)ink=true;
      }
      if((px-16)**2+(py-18)**2<3.2**2)ink=false;
    }else ink=leaves.some(points=>inside(px,py,points))||(px>=14&&px<18&&py>=16&&py<31);
    if(px>=4&&px<28&&py>=34&&py<37)ink=true;
    mask[y*width+x]=ink?1:0;
  }
  return mask;
}
