import test from 'node:test';
import assert from 'node:assert/strict';
import {flowerCatalog as nativeCatalog,flowerGeometry as nativeGeometry} from '../../../assets/models/flower-source.mjs';
import {flowerCatalog,flowerGeometry,previewOnlyFlowers} from '../models/flower-catalog.mjs';
import {parametricFlower,normalizeFlowerShape,flowerUV} from '../../../assets/models/parametric-flower.mjs';

function valid(model){
  assert.deepEqual(model.heads,[{id:0,anchor:[0,0,0],label:'完整花头'}]);
  let count=0;
  for(const part of model.parts){
    assert.equal(part.head,0);assert.equal(part.material,'palette');
    assert.ok(part.positions.every(Number.isFinite));assert.equal(part.positions.length%3,0);
    assert.equal(part.uvs.length,part.positions.length/3*2);assert.ok(part.uvs.every(v=>v>=0&&v<=1));
    assert.ok(part.indices.length>0);assert.equal(part.indices.length%3,0);
    assert.ok(part.indices.every(v=>Number.isInteger(v)&&v>=0&&v<part.positions.length/3));
    for(let i=0;i<part.indices.length;i+=3){
      const [a,b,c]=part.indices.slice(i,i+3).map(index=>part.positions.slice(index*3,index*3+3));
      const u=b.map((v,i)=>v-a[i]),v=c.map((n,i)=>n-a[i]);
      assert.ok(Math.hypot(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])>1e-10);
    }
    count+=part.indices.length/3;
  }
  assert.ok(count<3000);
}
test('all native and web presets use the same head generator and identical shared defaults',()=>{
  assert.equal(new Set(flowerCatalog.map(spec=>spec.id)).size,flowerCatalog.length);
  assert.deepEqual(previewOnlyFlowers.map(spec=>spec.id),['white-geranium','gillenia','four-petal','five-star','custom-flower']);
  for(const spec of flowerCatalog){
    assert.deepEqual(flowerGeometry(spec.id),parametricFlower(spec.defaults));valid(flowerGeometry(spec.id));
  }
  for(const spec of nativeCatalog){
    assert.deepEqual(flowerGeometry(spec.id),nativeGeometry(spec.id));
    assert.deepEqual(flowerCatalog.find(s=>s.id===spec.id).defaults,spec.defaults);
  }
  assert.equal(flowerCatalog.find(s=>s.id==='four-petal').defaults.petalCount,4);
  assert.equal(flowerCatalog.find(s=>s.id==='five-star').defaults.petalCount,5);
  assert.equal(flowerCatalog.find(s=>s.id==='forget-me-not').defaults.petalCount,5,'neither consumer inherits cosmos eight-petal cache template');
});
test('shape extremes remain finite and nondegenerate; changing palette never changes geometry',()=>{
  const original=parametricFlower();
  for(const [key,values]of Object.entries({petalCount:[3,24],flowerSize:[.65,1.3],petalWidth:[.08,.65],petalLength:[.6,1.4],tipSharpness:[0,1],notch:[.1,.25],opening:[-1,1],centerShape:['flat','cone'],centerRadius:[.04,.35],centerHeight:[0,.55],tilt:[-15,85]})){
    for(const value of values){const changed=parametricFlower({[key]:value});valid(changed);assert.notDeepEqual(changed,original,key);}
  }
  valid(parametricFlower({petalCount:24,petalWidth:.65,opening:1,notch:.25,tipSharpness:1,centerRadius:.04}));
  assert.deepEqual(parametricFlower({paletteA:'#ffffff',weightMap:'veins'}),original);
  assert.deepEqual(normalizeFlowerShape({petalCount:NaN,opening:Infinity}),normalizeFlowerShape());
});
test('flat, cupped and hanging petals differ independently of tip sharpness; center shapes and UV islands are explicit',()=>{
  const z=opening=>parametricFlower({opening,tilt:0}).parts[0].positions.filter((_,i)=>i%3===2);
  assert.ok(Math.max(...z(1))>.4);assert.ok(Math.min(...z(-1))<-.4);
  assert.ok(Math.max(...z(0))<.04);
  for(const island of ['petal','center','calyx'])for(const u of [0,1])for(const v of [0,1]){
    const [x,y]=flowerUV(island,u,v);
    assert.ok(x>0&&x<1&&y>0&&y<1);
    assert.equal(x<.75,island==='petal');
    if(island!=='petal')assert.equal(y<.5,island==='center');
  }
});
