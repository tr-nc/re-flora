// Original, continuous vector artwork. View-box units are not a raster/pixel grid.
// One large silhouette + one solid title block; no lettering, logos or brand assets.
const path=(commands,fill='ink')=>({kind:'path',commands,fill});
const layer=offset=>path([
  ['moveTo',10,27.5+offset],['bezierCurveTo',30,15+offset,42.5,32+offset,70,22.5+offset],
  ['lineTo',70,35+offset],['bezierCurveTo',42.5,43+offset,30,27.5+offset,10,40+offset],['closePath'],
]);

export function soilBagStickerArt(design='sprout'){
  let shapes;
  if(design==='layers')shapes=[0,17.5,35].map(layer);
  else if(design==='flower'){
    shapes=Array.from({length:5},(_,i)=>{
      const rotation=-Math.PI/2+i*Math.PI*2/5;
      return {kind:'ellipse',x:40+17.5*Math.cos(rotation),y:45+17.5*Math.sin(rotation),rx:15,ry:12,rotation,fill:'ink'};
    });
    shapes.push({kind:'ellipse',x:40,y:45,rx:8,ry:8,rotation:0,fill:'background'});
  }else shapes=[
    path([['moveTo',35,40],['bezierCurveTo',36,52,36,66,35,77.5],['lineTo',45,77.5],['bezierCurveTo',44,66,44,52,45,40],['closePath']]),
    path([['moveTo',40,47.5],['bezierCurveTo',20,50,10,37.5,10,20],['bezierCurveTo',27.5,17.5,42.5,27.5,40,47.5],['closePath']]),
    path([['moveTo',40,45],['bezierCurveTo',37.5,25,50,15,70,15],['bezierCurveTo',70,35,57.5,47.5,40,45],['closePath']]),
  ];
  shapes.push({kind:'rect',x:10,y:85,width:60,height:7.5,fill:'ink'});
  return {viewBox:{width:80,height:100},shapes};
}
