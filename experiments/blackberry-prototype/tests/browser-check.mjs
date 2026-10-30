// Explicit WebGL check, not part of the fast node:test suite. Requires agent-browser.
import {execFileSync} from 'node:child_process';
import {mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
const url=process.env.BLACKBERRY_URL||'http://127.0.0.1:8767/blackberry-prototype/';
const output=resolve('target/blackberry-prototype');mkdirSync(output,{recursive:true});
const env={...process.env,AGENT_BROWSER_SESSION:'reflora-blackberry-validation'};
function run(...args) {return execFileSync('agent-browser',args,{env,encoding:'utf8',stdio:['pipe','pipe','pipe']}).trim();}
function evaluate(code) {console.log(run('eval',`(async()=>{const assert=(ok,message)=>{if(!ok)throw new Error(message)};const frame=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));${code}})()`));}
try {
  console.log(run('open',url));run('set','viewport','1440','1100');run('wait','--fn','window.blackberryStudy?.ready === true');
  run('uncheck','#wind');
  evaluate(`
    const initial=window.blackberryStudy;
    assert(initial.counts.canes===11,'default cane count');
    assert(initial.counts.fruitSites>0,'fruit sites present');
    assert(initial.counts.triangles>1000,'real geometry rendered');
    assert(document.documentElement.scrollWidth<=innerWidth,'desktop overflow');
    const requests=performance.getEntriesByType('resource');
    assert(requests.every(r=>new URL(r.name).origin===location.origin),'unexpected external dependency');
    return {defaultCounts:initial.counts,renderSize:initial.renderSize(),externalRequests:0};
  `);
  run('screenshot',`${output}/desktop.png`);
  run('click','[data-view=top]');run('screenshot',`${output}/top.png`);
  run('click','[data-view=detail]');run('screenshot',`${output}/detail.png`);
  run('click','[data-view=side]');run('check','#skeleton');run('screenshot',`${output}/skeleton.png`);
  evaluate(`assert(window.blackberryStudy.settings.skeleton,'skeleton checkbox'); assert(document.querySelector('#model-stats').textContent.includes('0 簇'),'leaf layer hidden'); return 'skeleton passed';`);
  run('uncheck','#skeleton');run('click','[data-view=garden]');run('click','[data-stage=flower]');run('screenshot',`${output}/flower.png`);
  run('click','[data-stage=leaf]');
  evaluate(`assert(window.blackberryStudy.settings.stage==='leaf','stage switch'); return 'flower and leaf controls passed';`);
  run('click','#reset');run('uncheck','#wind');
  evaluate(`
    const set=(id,value)=>{const el=document.getElementById(id);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));};
    set('canes',17);assert(window.blackberryStudy.counts.canes===17,'cane control');
    set('spread',1.4);assert(window.blackberryStudy.settings.spread===1.4,'spread control');
    set('density',.2);const sparse=window.blackberryStudy.counts.leaves;
    set('density',1);assert(window.blackberryStudy.counts.leaves>sparse,'density control');
    document.getElementById('reset').click();await frame();
    const baseline=window.blackberryStudy.renderer();
    const leaves=window.blackberryStudy.counts.leaves;
    for(let i=0;i<12;i++){
      document.getElementById('reseed').click();await frame();
      document.getElementById('reset').click();await frame();
    }
    assert(window.blackberryStudy.settings.seed===27,'reset seed');
    assert(window.blackberryStudy.counts.leaves===leaves,'reset morphology');
    const final=window.blackberryStudy.renderer();
    assert(final.geometries===baseline.geometries,'geometry leak');
    assert(final.textures===baseline.textures,'texture leak');
    return {rebuildCycles:12,baseline,final};
  `);
  run('uncheck','#wind');run('uncheck','#scale');run('check','#pixel');
  evaluate(`await frame(); assert(window.blackberryStudy.renderSize().width<=300,'pixel buffer size'); assert(window.blackberryStudy.settings.scale===false,'scale toggle'); return {pixelSize:window.blackberryStudy.renderSize()};`);
  run('screenshot',`${output}/pixel.png`);run('click','#reset');
  run('set','viewport','390','844');
  evaluate(`await frame();assert(document.documentElement.scrollWidth<=innerWidth,'mobile overflow'); assert(document.querySelector('#scene').getBoundingClientRect().width<=innerWidth,'canvas overflow'); return {mobileWidth:innerWidth,scrollWidth:document.documentElement.scrollWidth};`);
  run('screenshot',`${output}/mobile.png`);run('screenshot','--full',`${output}/mobile-full.png`);
  run('click','[data-stage=flower]');
  evaluate(`assert(window.blackberryStudy.settings.stage==='flower','mobile stage control'); return 'mobile interaction passed';`);
  run('set','viewport','1440','1100');run('click','#reset');run('screenshot','--full',`${output}/desktop-full.png`);
  const errors=run('errors');if(errors)throw new Error(errors);
  console.log(`Browser checks passed. Screenshots: ${output}`);
} finally {run('close');}
