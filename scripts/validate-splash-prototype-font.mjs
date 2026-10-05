#!/usr/bin/env node
// Browser regressions: standalone game-font title, tracking and flower seams.
// Requires agent-browser and its Chromium installation; not a cargo test.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = new URL('../', import.meta.url);
const html = readFileSync(new URL('docs/research/splash-prototype.html', root), 'utf8');
const font = readFileSync(new URL('assets/font/PixelifySans-VariableFont_wght.ttf', root));
const embedded = html.match(/data:font\/ttf;base64,([A-Za-z0-9+/=]+)/);
assert.ok(embedded, 'Standalone HTML must embed its font.');
assert.deepEqual(Buffer.from(embedded[1], 'base64'), font, 'Embedded font must be the exact game font.');

const directory = mkdtempSync(join(tmpdir(), 're-flora-splash-font-'));
const session = execFileSync('agent-browser', ['session', 'id', '--scope', 'worktree', '--prefix', 'splash-font-regression'], { encoding: 'utf8' }).trim();
const browser = (...args) => execFileSync('agent-browser', ['--session', session, ...args], { encoding: 'utf8' });
try {
  const page = join(directory, 'index.html');
  writeFileSync(page, html); // No sibling assets: exercises the reported failure.
  browser('open', pathToFileURL(page).href);
  browser('wait', '--fn', 'width > 0 && document.fonts.status === "loaded"');
  console.log(browser('eval', `(() => {
    const check = (ok, message) => { if (!ok) throw Error(message); };
    check(fontReady && document.fonts.check('400 60px Pixelify'), 'Game font did not load in standalone HTML');
    state.motion = false; transitionTime = 0; replayStart = null;
    const original = ctx.fillText;
    const drawn = [];
    ctx.fillText = function(...args) { drawn.push(args[0]); return original.apply(this, args); };
    try {
      check(!$('titleSplit') && !('titleSplit' in state), 'Removed split-title mode must not remain');
      const widths = [];
      for (const spacing of [-2, 0, 6, 12]) {
        $('letterSpacing').value = spacing;
        $('letterSpacing').dispatchEvent(new Event('input'));
        drawn.length = 0; draw();
        check(drawn.join('') === 're: flora' && drawn.length === 1, 'Original wordmark must render as one text run');
        check(recipe().letterSpacing === spacing, 'Tracking not exported');
        const measured = trackedTitle('re: flora', spacing * layout().cell / 96).ink;
        widths.push(measured.right - measured.left);
        const l = layout(), x = Math.ceil((l.ox + l.titleColumn*l.cell)*dpr), y = Math.ceil((l.oy + l.titleRow*l.cell)*dpr);
        const image = ctx.getImageData(x, y, Math.floor(l.titleCells*l.cell*dpr)-1, Math.floor(l.cell*dpr)-1);
        const hex = state.colors.title.slice(1), color = [0,2,4].map(i => parseInt(hex.slice(i,i+2),16));
        let ink = 0;
        for (let i=0;i<image.data.length;i+=4) if(color.every((v,c)=>image.data[i+c]===v)) ink++;
        check(ink > 10, 'No visible title pixels at spacing ' + spacing);
      }
      check(widths.every((width, i) => i === 0 || width > widths[i-1]), 'Tracking slider must widen actual text');
      check(layout().titleCells === 4, 'Wordmark must keep the original four-cell region');
    } finally { ctx.fillText = original; }
    return { status: 'passed', standalone: true, tracking: [-2, 0, 6, 12], font: $('fontStatus').textContent };
  })()`));
  console.log(browser('eval', `(() => {
    const results = [];
    for (const degrees of [-10, 0, 10]) for (const size of [1, 1.5, 2, 3]) for (const alpha of [1, .5]) {
      const canvas = document.createElement('canvas'); canvas.width = 100; canvas.height = 100;
      const context = canvas.getContext('2d'), angle = degrees*Math.PI/180, center = 50.2;
      drawFlower(context, 'original-six', center, center, size, angle, state.colors.cream, alpha);
      const data = context.getImageData(0, 0, 100, 100).data;
      let minimum = 255;
      for (let y=40;y<60;y++) for (let x=40;x<60;x++) {
        const dx=x+.5-center, dy=y+.5-center;
        const u=(dx*Math.cos(angle)+dy*Math.sin(angle))/size, v=(-dx*Math.sin(angle)+dy*Math.cos(angle))/size;
        if(u*u+v*v<4) minimum=Math.min(minimum,data[(y*100+x)*4+3]);
      }
      if (minimum !== Math.round(255*alpha)) throw Error('Six-petal flower core opacity is inconsistent: angle='+degrees+', size='+size+', fade='+alpha+', alpha='+minimum);
      results.push({degrees,size,fade:alpha,minimumCoreAlpha:minimum});
    }
    const plum = flowerImage('plum', state.colors.cream).getContext('2d');
    for (const [x,y] of [[6,6],[9,6]]) {
      if (plum.getImageData(x*8+4,y*8+4,1,1).data[3] !== 255) throw Error('Plum has a transparent dark dot at '+x+','+y);
    }
    for (const shape of shapes) {
      const image = flowerImage(shape.id, state.colors.cream), local = image.getContext('2d');
      const authored = new Map(pixels(shape.id).map(p => [(p.y+8)*16+p.x+8, p]));
      for(let y=0;y<16;y++)for(let x=0;x<16;x++){
        const alpha=local.getImageData(x*8+4,y*8+4,1,1).data[3];
        if(alpha !== (authored.has(y*16+x)?255:0))throw Error('Local flower silhouette changed: '+shape.id);
      }
    }
    return {status:'passed',sixPetalCoverageCases:results.length,validatedSilhouettes:shapes.length,plumInteriorHolesFilled:2};
  })()`));
} finally {
  try { browser('close'); } finally { rmSync(directory, { recursive: true, force: true }); }
}
