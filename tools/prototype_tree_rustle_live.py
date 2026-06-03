#!/usr/bin/env python3
"""Browser Web Audio live tuner for procedural tree rustling.

Unlike ``prototype_tree_rustle.py`` and ``prototype_tree_rustle_gui.py``, this
script does not render WAV files. It serves a tiny local page whose AudioWorklet
calculates the rustle continuously at runtime, so slider changes are heard almost
immediately and the sound never loops.

Run:

    cd tools
    uv run python prototype_tree_rustle_live.py
"""

from __future__ import annotations

import argparse
import json
import textwrap
import webbrowser
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from socketserver import BaseServer
from typing import Final
from urllib.parse import urlparse

DEFAULT_HOST: Final = "127.0.0.1"
DEFAULT_PORT: Final = 8080

PRESETS: Final = {
    "soft": {
        "wind": 0.35,
        "gustiness": 0.35,
        "leafDensity": 0.70,
        "leafBody": 0.95,
        "crackle": 0.18,
        "brightness": 0.26,
        "dryness": 0.22,
        "air": 0.45,
        "branch": 0.02,
        "volume": 0.95,
    },
    "dense": {
        "wind": 0.62,
        "gustiness": 0.50,
        "leafDensity": 1.35,
        "leafBody": 0.88,
        "crackle": 0.34,
        "brightness": 0.38,
        "dryness": 0.36,
        "air": 0.62,
        "branch": 0.08,
        "volume": 0.90,
    },
    "dry": {
        "wind": 0.55,
        "gustiness": 0.55,
        "leafDensity": 1.05,
        "leafBody": 0.42,
        "crackle": 0.78,
        "brightness": 0.72,
        "dryness": 0.78,
        "air": 0.35,
        "branch": 0.05,
        "volume": 0.82,
    },
    "storm": {
        "wind": 0.88,
        "gustiness": 0.95,
        "leafDensity": 1.85,
        "leafBody": 0.78,
        "crackle": 0.62,
        "brightness": 0.58,
        "dryness": 0.52,
        "air": 1.00,
        "branch": 0.22,
        "volume": 0.78,
    },
    "less_plastic": {
        "wind": 0.58,
        "gustiness": 0.46,
        "leafDensity": 1.18,
        "leafBody": 1.08,
        "crackle": 0.16,
        "brightness": 0.22,
        "dryness": 0.20,
        "air": 0.55,
        "branch": 0.04,
        "volume": 0.95,
    },
}

SLIDERS: Final = [
    {
        "key": "wind",
        "label": "wind",
        "min": 0.0,
        "max": 1.0,
        "step": 0.01,
        "hint": "overall force; controls loudness, density, and brightness",
    },
    {
        "key": "gustiness",
        "label": "gustiness",
        "min": 0.0,
        "max": 1.0,
        "step": 0.01,
        "hint": "slow swell/release motion; high values feel stormy",
    },
    {
        "key": "leafDensity",
        "label": "leaf density",
        "min": 0.0,
        "max": 2.5,
        "step": 0.01,
        "hint": "number of overlapping leaf events",
    },
    {
        "key": "leafBody",
        "label": "leaf body",
        "min": 0.0,
        "max": 1.5,
        "step": 0.01,
        "hint": "warm low/mid rustle body; raise this if it sounds plasticky",
    },
    {
        "key": "crackle",
        "label": "crackle",
        "min": 0.0,
        "max": 1.0,
        "step": 0.01,
        "hint": "short high transients; lower this for less plastic bag",
    },
    {
        "key": "brightness",
        "label": "brightness",
        "min": 0.0,
        "max": 1.0,
        "step": 0.01,
        "hint": "high-frequency cutoff/shine",
    },
    {
        "key": "dryness",
        "label": "dryness",
        "min": 0.0,
        "max": 1.0,
        "step": 0.01,
        "hint": "dry leaves are sharper and more papery",
    },
    {
        "key": "air",
        "label": "air bed",
        "min": 0.0,
        "max": 1.5,
        "step": 0.01,
        "hint": "wide whoosh underneath the leaves",
    },
    {
        "key": "branch",
        "label": "branch creak",
        "min": 0.0,
        "max": 1.0,
        "step": 0.01,
        "hint": "rare low woody movement under stronger gusts",
    },
    {
        "key": "volume",
        "label": "volume",
        "min": 0.0,
        "max": 1.5,
        "step": 0.01,
        "hint": "post-synth gain; no normalization is applied in live mode",
    },
]


WORKLET_JS: Final = r"""
class TreeRustleProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.sr = sampleRate;
    this.params = {
      wind: 0.58,
      gustiness: 0.46,
      leafDensity: 1.18,
      leafBody: 1.08,
      crackle: 0.16,
      brightness: 0.22,
      dryness: 0.20,
      air: 0.55,
      branch: 0.04,
      volume: 0.95,
    };
    this.running = false;
    this.rngState = 0x12345678;
    this.controlRate = 100;
    this.controlSamples = Math.max(1, Math.round(this.sr / this.controlRate));
    this.controlCountdown = 0;
    this.wanderTarget = this.params.wind;
    this.wander = this.params.wind;
    this.windCurrent = this.params.wind;
    this.gusts = [];
    this.grains = [];
    this.creaks = [];

    this.airLpL = 0.0;
    this.airLpR = 0.0;
    this.airSlowL = 0.0;
    this.airSlowR = 0.0;
    this.bodyLpL = 0.0;
    this.bodyLpR = 0.0;
    this.bodySlowL = 0.0;
    this.bodySlowR = 0.0;
    this.leafHpL = 0.0;
    this.leafHpR = 0.0;
    this.leafLpL = 0.0;
    this.leafLpR = 0.0;

    this.airAlpha = 0.1;
    this.airSlowAlpha = 0.01;
    this.airAmp = 0.0;
    this.bodyAlpha = 0.1;
    this.bodySlowAlpha = 0.01;
    this.bodyAmp = 0.0;
    this.leafHpAlpha = 0.1;
    this.leafLpAlpha = 0.1;
    this.leafAmp = 0.0;
    this.burstRate = 0.0;
    this.branchRate = 0.0;

    this.port.onmessage = (event) => {
      const message = event.data || {};
      if (message.type === 'params') {
        Object.assign(this.params, message.params || {});
      } else if (message.type === 'running') {
        this.running = Boolean(message.running);
      } else if (message.type === 'seed') {
        this.setSeed(message.seed || 1);
      }
    };
  }

  clamp(value, low, high) {
    return Math.min(high, Math.max(low, value));
  }

  setSeed(seed) {
    const mixed = (Number(seed) >>> 0) || 1;
    this.rngState = mixed ^ 0x9e3779b9;
    this.wanderTarget = this.params.wind;
    this.wander = this.params.wind;
    this.windCurrent = this.params.wind;
    this.gusts.length = 0;
    this.grains.length = 0;
    this.creaks.length = 0;
  }

  rand() {
    let x = this.rngState >>> 0;
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    this.rngState = x >>> 0;
    return (this.rngState + 0.5) / 4294967296.0;
  }

  randRange(low, high) {
    return low + (high - low) * this.rand();
  }

  randInt(maxExclusive) {
    return Math.floor(this.rand() * Math.max(1, maxExclusive));
  }

  alpha(cutoffHz) {
    const cutoff = this.clamp(cutoffHz, 1.0, this.sr * 0.45);
    return 1.0 - Math.exp(-Math.PI * 2.0 * cutoff / this.sr);
  }

  equalPowerPan(pan) {
    const angle = (this.clamp(pan, -1.0, 1.0) + 1.0) * Math.PI * 0.25;
    return [Math.cos(angle), Math.sin(angle)];
  }

  updateControl() {
    const p = this.params;
    const wind = this.clamp(p.wind, 0.0, 1.0);
    const gustiness = this.clamp(p.gustiness, 0.0, 1.0);
    const dryness = this.clamp(p.dryness, 0.0, 1.0);
    const brightness = this.clamp(p.brightness, 0.0, 1.0);
    const leafDensity = Math.max(0.0, p.leafDensity);
    const crackle = this.clamp(p.crackle, 0.0, 1.0);

    this.wanderTarget += this.randRange(-0.028, 0.028) * gustiness;
    this.wanderTarget += (wind - this.wanderTarget) * 0.015;
    this.wanderTarget = this.clamp(
      this.wanderTarget,
      wind * 0.45,
      Math.min(1.0, wind + 0.22 + 0.25 * gustiness),
    );
    this.wander += (this.wanderTarget - this.wander) * 0.045;

    if (this.rand() < (0.05 + 0.23 * gustiness) / this.controlRate) {
      this.gusts.push({
        age: 0.0,
        duration: this.randRange(0.65, 2.8 + 1.4 * gustiness),
        amp: this.randRange(0.12, 0.50) * gustiness,
      });
    }

    let target = 0.72 * wind + 0.28 * this.wander;
    const nextGusts = [];
    for (const gust of this.gusts) {
      gust.age += 1.0 / this.controlRate;
      const t = gust.age / Math.max(0.001, gust.duration);
      if (t < 1.0) {
        const shape = Math.pow(Math.sin(Math.PI * t), 1.35);
        const shoulder = Math.pow(Math.sin(Math.PI * Math.min(1.0, t * 1.8)), 2.0) * (1.0 - t) * 0.22;
        target += gust.amp * (shape + shoulder);
        nextGusts.push(gust);
      }
    }
    this.gusts = nextGusts;

    target = this.clamp(target, 0.0, 1.0);
    const smoothAlpha = target >= this.windCurrent ? 0.12 : 0.042;
    this.windCurrent += (target - this.windCurrent) * smoothAlpha;
    const w = this.clamp(this.windCurrent, 0.0, 1.0);
    const windLift = Math.pow(w, 1.35);

    this.airAlpha = this.alpha(700.0 + 1450.0 * w);
    this.airSlowAlpha = this.alpha(90.0 + 70.0 * w);
    this.airAmp = 0.115 * Math.max(0.0, p.air) * (0.16 + windLift);

    this.bodyAlpha = this.alpha(520.0 + 1350.0 * w + 650.0 * brightness);
    this.bodySlowAlpha = this.alpha(70.0 + 80.0 * w);
    this.bodyAmp =
      0.060 *
      Math.max(0.0, p.leafBody) *
      leafDensity *
      Math.pow(w, 1.55) *
      (1.10 - 0.38 * dryness);

    this.leafHpAlpha = this.alpha(430.0 + 680.0 * dryness + 620.0 * brightness + 480.0 * w);
    this.leafLpAlpha = this.alpha(
      2800.0 + 3400.0 * brightness + 2600.0 * dryness + 2300.0 * w,
    );
    this.leafAmp =
      0.035 *
      leafDensity *
      Math.pow(w, 1.85) *
      (0.55 + 0.60 * dryness) *
      (0.45 + 0.80 * brightness);

    this.burstRate = (0.55 + 23.0 * Math.pow(w, 2.25)) * leafDensity * (0.35 + 1.10 * crackle);
    this.branchRate = Math.max(0.0, p.branch) * Math.pow(Math.max(0.0, w - 0.42), 2.0) * 1.15;
  }

  makeGrain(delaySamples) {
    const p = this.params;
    const dryness = this.clamp(p.dryness, 0.0, 1.0);
    const brightness = this.clamp(p.brightness, 0.0, 1.0);
    const crackle = this.clamp(p.crackle, 0.0, 1.0);
    const wind = this.clamp(this.windCurrent, 0.0, 1.0);

    let duration = this.randRange(0.022, 0.118 - 0.032 * dryness);
    duration *= 1.20 - 0.45 * crackle;
    duration = Math.max(0.010, duration);
    const decay = Math.exp(-1.0 / (duration * this.sr));
    const attackMs = this.randRange(2.5, 11.0) * (1.15 - 0.45 * crackle);
    const attackAlpha = 1.0 - Math.exp(-1.0 / (attackMs * 0.001 * this.sr));
    const hp = this.randRange(
      420.0 + 520.0 * dryness + 650.0 * brightness,
      1450.0 + 1350.0 * dryness + 2200.0 * brightness,
    );
    const lp = this.randRange(
      3100.0 + 1700.0 * dryness + 2300.0 * brightness,
      6100.0 + 2700.0 * dryness + 4300.0 * brightness,
    );
    const amp =
      this.randRange(0.010, 0.054) *
      (0.45 + wind) *
      (0.72 + 0.38 * dryness) *
      (0.35 + 1.05 * crackle);
    const pan = this.equalPowerPan(this.randRange(-0.92, 0.92));

    return {
      delay: delaySamples,
      env: 0.0,
      target: amp,
      attackAlpha,
      decay,
      hpState: 0.0,
      hpAlpha: this.alpha(hp),
      lpState: 0.0,
      lpAlpha: this.alpha(lp),
      panL: pan[0],
      panR: pan[1],
    };
  }

  makeCreak() {
    const wind = this.clamp(this.windCurrent, 0.0, 1.0);
    const branch = Math.max(0.0, this.params.branch);
    const duration = this.randRange(0.35, 1.25);
    const pan = this.equalPowerPan(this.randRange(-0.65, 0.65));
    return {
      env: this.randRange(0.035, 0.090) * wind * branch,
      decay: Math.exp(-1.0 / (duration * this.sr)),
      phase: this.randRange(0.0, Math.PI * 2.0),
      wobblePhase: this.randRange(0.0, Math.PI * 2.0),
      frequency: this.randRange(75.0, 210.0),
      wobbleFrequency: this.randRange(1.1, 4.7),
      panL: pan[0],
      panR: pan[1],
      noiseLp: 0.0,
    };
  }

  softLimit(value) {
    return Math.tanh(value * 1.75) / 1.75;
  }

  process(_inputs, outputs) {
    const output = outputs[0];
    const left = output[0];
    const right = output[1] || output[0];

    if (!this.running) {
      left.fill(0.0);
      if (right !== left) right.fill(0.0);
      return true;
    }

    for (let i = 0; i < left.length; i += 1) {
      if (this.controlCountdown <= 0) {
        this.updateControl();
        this.controlCountdown = this.controlSamples;
      }
      this.controlCountdown -= 1;

      const p = this.params;
      const wind = this.clamp(this.windCurrent, 0.0, 1.0);
      const crackle = this.clamp(p.crackle, 0.0, 1.0);
      let outL = 0.0;
      let outR = 0.0;

      let rawL = this.randRange(-1.0, 1.0);
      let rawR = this.randRange(-1.0, 1.0);
      this.airLpL += (rawL - this.airLpL) * this.airAlpha;
      this.airLpR += (rawR - this.airLpR) * this.airAlpha;
      this.airSlowL += (this.airLpL - this.airSlowL) * this.airSlowAlpha;
      this.airSlowR += (this.airLpR - this.airSlowR) * this.airSlowAlpha;
      outL += (this.airLpL - 0.78 * this.airSlowL) * this.airAmp;
      outR += (this.airLpR - 0.78 * this.airSlowR) * this.airAmp;

      rawL = this.randRange(-1.0, 1.0);
      rawR = this.randRange(-1.0, 1.0);
      this.bodyLpL += (rawL - this.bodyLpL) * this.bodyAlpha;
      this.bodyLpR += (rawR - this.bodyLpR) * this.bodyAlpha;
      this.bodySlowL += (this.bodyLpL - this.bodySlowL) * this.bodySlowAlpha;
      this.bodySlowR += (this.bodyLpR - this.bodySlowR) * this.bodySlowAlpha;
      outL += (this.bodyLpL - 0.84 * this.bodySlowL) * this.bodyAmp;
      outR += (this.bodyLpR - 0.84 * this.bodySlowR) * this.bodyAmp;

      rawL = this.randRange(-1.0, 1.0);
      rawR = this.randRange(-1.0, 1.0);
      this.leafHpL += (rawL - this.leafHpL) * this.leafHpAlpha;
      this.leafHpR += (rawR - this.leafHpR) * this.leafHpAlpha;
      const highL = rawL - this.leafHpL;
      const highR = rawR - this.leafHpR;
      this.leafLpL += (highL - this.leafLpL) * this.leafLpAlpha;
      this.leafLpR += (highR - this.leafLpR) * this.leafLpAlpha;
      outL += this.leafLpL * this.leafAmp;
      outR += this.leafLpR * this.leafAmp;

      if (this.rand() < this.burstRate / this.sr) {
        let clusterCount = 1 + this.randInt(1 + Math.floor(1 + 5 * wind * (0.55 + crackle)));
        if (this.rand() < (0.10 + 0.28 * wind) * (0.45 + crackle)) {
          clusterCount += 1 + this.randInt(4);
        }
        const clusterWindow = Math.floor(this.sr * this.randRange(0.010, 0.070 + 0.035 * wind));
        for (let g = 0; g < clusterCount; g += 1) {
          this.grains.push(this.makeGrain(this.randInt(Math.max(1, clusterWindow))));
        }
      }

      if (this.rand() < this.branchRate / this.sr) {
        this.creaks.push(this.makeCreak());
      }

      const nextGrains = [];
      for (const grain of this.grains) {
        if (grain.delay > 0) {
          grain.delay -= 1;
          nextGrains.push(grain);
          continue;
        }
        grain.target *= grain.decay;
        grain.env += (grain.target - grain.env) * grain.attackAlpha;
        const raw = this.randRange(-1.0, 1.0);
        grain.hpState += (raw - grain.hpState) * grain.hpAlpha;
        const high = raw - grain.hpState;
        grain.lpState += (high - grain.lpState) * grain.lpAlpha;
        const sample = grain.lpState * grain.env;
        outL += sample * grain.panL;
        outR += sample * grain.panR;
        if (grain.env > 0.00005 || grain.target > 0.00005) {
          nextGrains.push(grain);
        }
      }
      this.grains = nextGrains;

      const nextCreaks = [];
      for (const creak of this.creaks) {
        creak.env *= creak.decay;
        creak.wobblePhase += (Math.PI * 2.0 * creak.wobbleFrequency) / this.sr;
        const wobble = 1.0 + 0.11 * Math.sin(creak.wobblePhase) + 0.035 * this.randRange(-1.0, 1.0);
        creak.phase += (Math.PI * 2.0 * creak.frequency * wobble) / this.sr;
        creak.noiseLp += (this.randRange(-1.0, 1.0) - creak.noiseLp) * 0.018;
        const tone = Math.sin(creak.phase) + 0.35 * Math.sin(creak.phase * 2.03 + 0.7);
        const sample = (0.72 * tone + 0.28 * creak.noiseLp) * creak.env;
        outL += sample * creak.panL;
        outR += sample * creak.panR;
        if (creak.env > 0.00003) {
          nextCreaks.push(creak);
        }
      }
      this.creaks = nextCreaks;

      const gain = Math.max(0.0, p.volume);
      left[i] = this.softLimit(outL * gain);
      right[i] = this.softLimit(outR * gain);
    }

    return true;
  }
}

registerProcessor('tree-rustle', TreeRustleProcessor);
"""

HTML_TEMPLATE: Final = r"""
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Live Tree Rustle Tuner</title>
  <style>
    :root { color-scheme: light dark; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; background: #101512; color: #eaf3ec; }
    main { max-width: 1180px; margin: 0 auto; padding: 24px; }
    h1 { margin: 0 0 6px; font-size: 30px; }
    .subtle { color: #a8b8ad; }
    .panel { background: #18221b; border: 1px solid #2e4034; border-radius: 16px; padding: 18px; box-shadow: 0 8px 32px #0007; }
    .grid { display: grid; grid-template-columns: minmax(0, 1fr) 340px; gap: 18px; align-items: start; }
    .row { display: grid; grid-template-columns: 132px minmax(160px, 1fr) 72px minmax(180px, 300px); gap: 12px; align-items: center; margin: 14px 0; }
    label { color: #d7e5da; font-size: 14px; }
    input[type="range"] { width: 100%; accent-color: #78d486; }
    input[type="number"] { width: 70px; background: #0f1511; color: #eef9f0; border: 1px solid #3a5140; border-radius: 8px; padding: 6px; }
    button, select { background: #203328; color: #f1fff4; border: 1px solid #43614b; border-radius: 10px; padding: 10px 12px; font: inherit; }
    button { cursor: pointer; transition: background 120ms ease, transform 120ms ease; }
    button:hover { background: #2a4934; }
    button:active { transform: translateY(1px); }
    button.primary { background: #2c7c3a; border-color: #5abc68; font-weight: 700; }
    button.stop { background: #71302c; border-color: #c0665c; }
    .button-row { display: flex; flex-wrap: wrap; gap: 10px; margin: 12px 0; }
    .preset { text-transform: capitalize; }
    .value { color: #b7eebd; font-variant-numeric: tabular-nums; text-align: right; }
    .hint { color: #9aae9f; font-size: 12px; }
    .status { background: #0f1511; border: 1px solid #334535; border-radius: 12px; padding: 12px; min-height: 70px; white-space: pre-wrap; }
    textarea { box-sizing: border-box; width: 100%; min-height: 190px; resize: vertical; background: #0f1511; color: #d7ffe0; border: 1px solid #334535; border-radius: 12px; padding: 12px; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }
    .meter { height: 10px; border-radius: 999px; background: #0d120f; overflow: hidden; border: 1px solid #334535; }
    .meter > div { height: 100%; width: 0%; background: linear-gradient(90deg, #5fc46a, #d4bd4d); }
    .note { padding: 12px; background: #151c17; border-left: 4px solid #78d486; border-radius: 8px; }
    @media (max-width: 900px) { .grid { grid-template-columns: 1fr; } .row { grid-template-columns: 112px 1fr 64px; } .hint { grid-column: 2 / 4; } }
  </style>
</head>
<body>
  <main>
    <h1>Live Procedural Tree Rustle</h1>
    <div class="subtle">No WAV rendering, no loop. Audio is calculated continuously in a browser AudioWorklet; sliders stream into the synth instantly.</div>

    <div class="grid" style="margin-top: 20px;">
      <section class="panel">
        <h2 style="margin-top: 0;">Sliders</h2>
        <div id="sliders"></div>
      </section>

      <aside class="panel">
        <h2 style="margin-top: 0;">Transport</h2>
        <div class="button-row">
          <button id="start" class="primary">Start audio</button>
          <button id="stop" class="stop">Stop</button>
        </div>
        <label for="seed">seed</label>
        <div class="button-row">
          <input id="seed" type="number" value="3" min="1" max="999999" step="1" />
          <button id="apply-seed">Apply seed</button>
          <button id="random-seed">Randomize</button>
        </div>
        <div class="button-row" id="preset-buttons"></div>
        <div class="note subtle">
          Less plastic: lower <b>crackle</b>, <b>brightness</b>, and <b>dryness</b>; raise <b>leaf body</b>.
        </div>
        <h3>Level</h3>
        <div class="meter"><div id="meter-fill"></div></div>
        <h3>Status</h3>
        <div id="status" class="status">Ready. Click Start audio; browsers require a user gesture before sound can play.</div>
      </aside>
    </div>

    <section class="panel" style="margin-top: 18px;">
      <h2 style="margin-top: 0;">Current settings</h2>
      <textarea id="settings" readonly></textarea>
    </section>
  </main>

  <script>
    const PRESETS = __PRESETS__;
    const SLIDERS = __SLIDERS__;
    const state = { ...PRESETS.less_plastic };
    let context = null;
    let node = null;
    let analyser = null;
    let raf = null;

    const sliderRoot = document.getElementById('sliders');
    const statusEl = document.getElementById('status');
    const settingsEl = document.getElementById('settings');
    const meterFill = document.getElementById('meter-fill');
    const seedInput = document.getElementById('seed');

    function setStatus(text) {
      statusEl.textContent = text;
    }

    function postParams() {
      if (node) {
        node.port.postMessage({ type: 'params', params: state });
      }
      settingsEl.value = JSON.stringify(state, null, 2);
    }

    function setSliderValue(key, value) {
      const slider = document.getElementById(`slider-${key}`);
      const number = document.getElementById(`number-${key}`);
      const shown = Number(value);
      state[key] = shown;
      if (slider) slider.value = shown;
      if (number) number.value = shown.toFixed(2);
    }

    function applyPreset(name) {
      const preset = PRESETS[name];
      for (const [key, value] of Object.entries(preset)) {
        setSliderValue(key, value);
      }
      postParams();
      setStatus(`Applied preset: ${name}\n${name === 'less_plastic' ? 'Warm body up, crackle/brightness down.' : 'Tweak while audio is running for instant feedback.'}`);
    }

    function buildSliders() {
      for (const spec of SLIDERS) {
        const row = document.createElement('div');
        row.className = 'row';

        const label = document.createElement('label');
        label.textContent = spec.label;
        label.htmlFor = `slider-${spec.key}`;
        row.appendChild(label);

        const slider = document.createElement('input');
        slider.id = `slider-${spec.key}`;
        slider.type = 'range';
        slider.min = spec.min;
        slider.max = spec.max;
        slider.step = spec.step;
        slider.value = state[spec.key];
        row.appendChild(slider);

        const number = document.createElement('input');
        number.id = `number-${spec.key}`;
        number.type = 'number';
        number.min = spec.min;
        number.max = spec.max;
        number.step = spec.step;
        number.value = Number(state[spec.key]).toFixed(2);
        number.className = 'value';
        row.appendChild(number);

        const hint = document.createElement('div');
        hint.className = 'hint';
        hint.textContent = spec.hint;
        row.appendChild(hint);

        function update(value) {
          const clamped = Math.min(spec.max, Math.max(spec.min, Number(value)));
          setSliderValue(spec.key, clamped);
          postParams();
        }
        slider.addEventListener('input', () => update(slider.value));
        number.addEventListener('input', () => update(number.value));

        sliderRoot.appendChild(row);
      }
    }

    function buildPresetButtons() {
      const root = document.getElementById('preset-buttons');
      for (const name of Object.keys(PRESETS)) {
        const button = document.createElement('button');
        button.className = 'preset';
        button.textContent = name.replace('_', ' ');
        button.addEventListener('click', () => applyPreset(name));
        root.appendChild(button);
      }
    }

    async function ensureAudio() {
      if (!context) {
        context = new AudioContext();
        await context.audioWorklet.addModule('/tree-rustle-worklet.js');
        node = new AudioWorkletNode(context, 'tree-rustle', {
          numberOfInputs: 0,
          numberOfOutputs: 1,
          outputChannelCount: [2],
        });
        analyser = context.createAnalyser();
        analyser.fftSize = 512;
        node.connect(analyser);
        analyser.connect(context.destination);
        postParams();
        node.port.postMessage({ type: 'seed', seed: Number(seedInput.value) || 1 });
      }
      if (context.state !== 'running') {
        await context.resume();
      }
    }

    function startMeter() {
      if (!analyser) return;
      const data = new Float32Array(analyser.fftSize);
      const tick = () => {
        analyser.getFloatTimeDomainData(data);
        let sum = 0.0;
        for (let i = 0; i < data.length; i += 1) sum += data[i] * data[i];
        const rms = Math.sqrt(sum / data.length);
        const pct = Math.min(100, rms * 950);
        meterFill.style.width = `${pct}%`;
        raf = requestAnimationFrame(tick);
      };
      if (raf) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(tick);
    }

    document.getElementById('start').addEventListener('click', async () => {
      try {
        await ensureAudio();
        node.port.postMessage({ type: 'running', running: true });
        postParams();
        startMeter();
        setStatus(`Running at ${Math.round(context.sampleRate)} Hz. Move sliders for live feedback.\nNo render buffer and no loop are used.`);
      } catch (error) {
        setStatus(`Audio start failed: ${error}`);
        console.error(error);
      }
    });

    document.getElementById('stop').addEventListener('click', () => {
      if (node) node.port.postMessage({ type: 'running', running: false });
      setStatus('Stopped. The audio graph remains loaded for fast restart.');
    });

    document.getElementById('apply-seed').addEventListener('click', () => {
      if (node) node.port.postMessage({ type: 'seed', seed: Number(seedInput.value) || 1 });
      setStatus(`Applied seed ${seedInput.value}; stochastic state reset.`);
    });

    document.getElementById('random-seed').addEventListener('click', () => {
      seedInput.value = String(1 + Math.floor(Math.random() * 999999));
      if (node) node.port.postMessage({ type: 'seed', seed: Number(seedInput.value) || 1 });
      setStatus(`Randomized seed to ${seedInput.value}.`);
    });

    buildSliders();
    buildPresetButtons();
    postParams();
  </script>
</body>
</html>
"""


def html() -> str:
    return HTML_TEMPLATE.replace("__PRESETS__", json.dumps(PRESETS)).replace(
        "__SLIDERS__", json.dumps(SLIDERS)
    )


class RustleRequestHandler(BaseHTTPRequestHandler):
    server: BaseServer

    def log_message(self, format: str, *args: object) -> None:  # noqa: A002 - stdlib signature
        return

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        if path in {"/", "/index.html"}:
            self.write_response("text/html; charset=utf-8", html())
            return
        if path == "/tree-rustle-worklet.js":
            self.write_response("text/javascript; charset=utf-8", WORKLET_JS)
            return
        self.send_error(HTTPStatus.NOT_FOUND, "not found")

    def write_response(self, content_type: str, body: str) -> None:
        encoded = body.encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(encoded)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Open a live Web Audio tree-rustle tuner.")
    parser.add_argument("--host", default=DEFAULT_HOST)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--no-open", action="store_true", help="do not open the browser automatically")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    server = ThreadingHTTPServer((args.host, args.port), RustleRequestHandler)
    host, port = server.server_address[:2]
    url = f"http://{host}:{port}"
    print(
        textwrap.dedent(f"""
        Live tree-rustle tuner running:
          {url}

        Click Start audio in the browser. Move sliders while it plays for instant feedback.
        Press Ctrl+C here to stop the server.
        """).strip(),
        flush=True,
    )
    if not args.no_open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nstopping live tuner")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
