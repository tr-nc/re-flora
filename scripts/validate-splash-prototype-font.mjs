#!/usr/bin/env node
// Browser regression: standalone game-font title and adjustable tracking.
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
} finally {
  try { browser('close'); } finally { rmSync(directory, { recursive: true, force: true }); }
}
