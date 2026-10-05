#!/usr/bin/env node
// Bundle the exact game font so the art prototype can be copied/viewed alone.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const usage = 'Usage: node scripts/build-splash-prototype.mjs [--check]\nBuild standalone docs/research/splash-prototype.html; --check verifies it is current.';
const args = process.argv.slice(2);
if (args.includes('--help')) {
  console.log(usage);
} else if (args.length > 1 || args.some(arg => arg !== '--check')) {
  console.error(usage);
  process.exitCode = 2;
} else {
  const root = new URL('../', import.meta.url);
  const templatePath = new URL('docs/research/splash-prototype.template.html', root);
  const outputPath = new URL('docs/research/splash-prototype.html', root);
  const fontPath = new URL('assets/font/PixelifySans-VariableFont_wght.ttf', root);
  const template = readFileSync(templatePath, 'utf8');
  const marker = '__PIXELIFY_FONT_DATA_URL__';
  if (template.split(marker).length !== 2) {
    throw new Error('Splash template must contain exactly one font marker.');
  }
  const font = readFileSync(fontPath).toString('base64');
  const html = template.replace(marker, `data:font/ttf;base64,${font}`);
  if (args.includes('--check')) {
    if (readFileSync(outputPath, 'utf8') !== html) {
      console.error('Splash HTML is stale. Run: node scripts/build-splash-prototype.mjs');
      process.exitCode = 1;
    } else {
      console.log('Splash HTML is current; game font is embedded.');
    }
  } else {
    writeFileSync(outputPath, html);
    console.log(`Built ${fileURLToPath(outputPath)} with the embedded game font.`);
  }
}
