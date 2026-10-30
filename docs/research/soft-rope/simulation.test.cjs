const test = require('node:test');
const assert = require('node:assert/strict');
const { RopeSimulation, PRESETS, GROUND, ROCK } = require('./simulation.js');

function run(sim, seconds, fps = 60) {
  for (let i = 0; i < seconds * fps; i++) sim.advance(1 / fps);
}
function assertContacts(sim, tolerance = 0.5) {
  const radius = sim.settings.radius;
  for (const p of sim.points) assert.ok(p.y <= GROUND - radius + tolerance, 'rope stays above the floor');
  for (let i = 0; i < sim.points.length - 1; i++) {
    const a = sim.points[i], b = sim.points[i + 1];
    const dx = b.x - a.x, dy = b.y - a.y;
    const t = Math.max(0, Math.min(1, ((ROCK.x - a.x) * dx + (ROCK.y - a.y) * dy) / (dx * dx + dy * dy)));
    assert.ok(Math.hypot(a.x + t * dx - ROCK.x, a.y + t * dy - ROCK.y) >= ROCK.radius + radius - tolerance, 'whole segment clears the rock');
  }
}

for (const mode of Object.keys(PRESETS)) {
  test(`${mode}: settled preset is finite, attached and nearly inextensible`, () => {
    const sim = new RopeSimulation(mode);
    run(sim, 12);
    assert.ok(sim.diagnostics().finite);
    assert.ok(sim.diagnostics().maxStrain < 0.04, JSON.stringify(sim.diagnostics()));
    assert.equal(sim.points[0].x, PRESETS[mode].start[0]);
    assert.equal(sim.points[0].y, PRESETS[mode].start[1]);
    if (PRESETS[mode].pin) {
      assert.equal(sim.points.at(-1).x, PRESETS[mode].end[0]);
      assert.equal(sim.points.at(-1).y, PRESETS[mode].end[1]);
    }
    assertContacts(sim);
  });
}

test('30, 60 and 144 Hz render grouping produces the same fixed-step result', () => {
  const states = [30, 60, 144].map(fps => {
    const sim = new RopeSimulation('kite');
    run(sim, 4, fps);
    return sim;
  });
  for (const sim of states.slice(1)) {
    assert.equal(sim.time, states[0].time);
    assert.deepEqual(sim.points, states[0].points);
  }
});

test('wind supports a free kite; still air lets it fall', () => {
  const windy = new RopeSimulation('kite'), calm = new RopeSimulation('kite');
  calm.settings.wind = 0;
  run(windy, 15); run(calm, 15);
  assert.ok(windy.points.at(-1).y < 350);
  assert.ok(calm.points.at(-1).y > 450);
  assert.ok(calm.diagnostics().finite);
});

test('bend stiffness changes the hose shape, not its authored length', () => {
  const soft = new RopeSimulation('hose'), stiff = new RopeSimulation('hose');
  soft.settings.bend = 0; stiff.settings.bend = 100;
  run(soft, 15); run(stiff, 15);
  assert.ok(soft.points[24].y - stiff.points[24].y > 80);
  assert.equal(soft.length, stiff.length);
  assert.ok(soft.diagnostics().maxStrain < 0.04);
  assert.ok(stiff.diagnostics().maxStrain < 0.04);
});

test('drag and release use the same public interaction path', () => {
  const sim = new RopeSimulation('hemp');
  run(sim, 4);
  const middle = sim.points[24];
  const originalY = middle.y;
  assert.equal(sim.beginDrag(middle.x, middle.y), 24);
  sim.moveDrag(middle.x, 230);
  run(sim, 2);
  assert.ok(middle.y < originalY - 60);
  sim.endDrag();
  run(sim, 6);
  assert.ok(middle.y > originalY - 10);
  assert.ok(sim.diagnostics().maxStrain < 0.04);
});

test('unpinning releases the endpoint and repinning holds its current pose', () => {
  const sim = new RopeSimulation('hemp');
  run(sim, 2); sim.setPinned(false); run(sim, 4);
  assert.ok(sim.points.at(-1).y > 250);
  sim.setPinned(true);
  const { x, y } = sim.points.at(-1);
  run(sim, 2);
  assert.equal(sim.points.at(-1).x, x);
  assert.equal(sim.points.at(-1).y, y);
});

test('rest-length adjustment is gradual and bounded by pinned span', () => {
  const sim = new RopeSimulation('hemp');
  run(sim, 2);
  sim.settings.length = 950;
  sim.advance(1 / 60);
  assert.ok(sim.length > 750 && sim.length < 752);
  run(sim, 4);
  assert.ok(Math.abs(sim.length - 950) < 1e-6);
  sim.settings.length = 100;
  run(sim, 8);
  assert.ok(sim.length >= Math.hypot(630, 0));
  assert.ok(sim.diagnostics().finite);
});

test('moving a pin cannot demand a span longer than the rope', () => {
  const sim = new RopeSimulation('hemp');
  sim.beginDrag(sim.points[0].x, sim.points[0].y);
  sim.moveDrag(-200, 1000);
  run(sim, 4);
  const a = sim.points[0], b = sim.points.at(-1);
  assert.ok(Math.hypot(a.x - b.x, a.y - b.y) <= sim.length);
  assert.ok(sim.diagnostics().finite);
});

test('background catch-up is bounded; invalid elapsed times do not poison state', () => {
  const sim = new RopeSimulation();
  for (const delta of [NaN, Infinity, -1, 0]) sim.advance(delta);
  assert.equal(sim.time, 0);
  sim.advance(60);
  assert.ok(sim.time <= 0.10001);
  assert.ok(sim.droppedTime > 59);
  assert.ok(sim.diagnostics().finite);
});

test('gust expires and reset returns to the selected material', () => {
  const sim = new RopeSimulation('hose');
  sim.gust(); run(sim, 0.5);
  assert.ok(sim.wind() > sim.settings.wind / 100);
  run(sim, 2);
  assert.equal(sim.wind(), sim.settings.wind / 100);
  sim.settings.bend = 1; sim.setPinned(false); sim.reset();
  assert.equal(sim.mode, 'hose');
  assert.equal(sim.settings.bend, PRESETS.hose.bend);
  assert.equal(sim.settings.pin, true);
  assert.equal(sim.time, 0);
});
