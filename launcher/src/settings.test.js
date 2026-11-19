import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { ROWS, TABS, enabled, restoreWrites, rowsOf, toWrite, valueOf } from './settings.js';

const row = (key) => ROWS.find((r) => r.key === key);
const get = (key, ini = {}, prefs = {}) => valueOf(row(key), ini, prefs);

test('every row sits in a tab, has a unique key and a default the page can show', () => {
  const tabs = TABS.map(([id]) => id);
  assert.equal(new Set(ROWS.map((r) => r.key)).size, ROWS.length);
  for (const r of ROWS) {
    assert.ok(tabs.includes(r.tab), r.key);
    assert.ok(['toggle', 'choice', 'slider'].includes(r.kind), r.key);
    assert.ok(r.label, r.key);
    if (r.needs) assert.ok(ROWS.some((o) => o.key === r.needs.key), `${r.key} needs a row that exists`);
    if (r.kind === 'toggle') assert.match(r.default, /^[01]$/, r.key);
    if (r.kind === 'choice') assert.ok(r.options.some(([v]) => v === valueOf(r, {}, {})), `${r.key}: its default is shown as an option`);
    if (r.kind === 'slider') assert.equal(valueOf(r, {}, {}), r.default, r.key);
  }
});

test('the keys and values are the ones runtime/settings.c reads', () => {
  const c = readFileSync(new URL('../../runtime/settings.c', import.meta.url), 'utf8');
  for (const r of ROWS.filter((r) => !r.pref)) {
    assert.ok(c.includes(`"${r.key}"`), `${r.key} is a key settings.c reads`);
    for (const [v] of r.kind === 'choice' ? r.options : []) {
      if (/^\d+$/.test(v) || (r.key === 'resolution' && /^\dx$/.test(v))) continue; // read with atoi, or built with "%dx"
      assert.ok(c.includes(`"${v}"`), `${r.key} = ${v}`);
    }
  }
});

test('a missing key or an auto default is read the way the game reads it', () => {
  assert.equal(get('resolution'), 'match');
  assert.equal(get('resolution', { resolution: 'auto' }), 'match');
  assert.equal(get('resolution', { resolution: '67%' }), '75%');
  assert.equal(get('resolution', { resolution: '8x' }), '4x');
  assert.equal(get('resolution', { resolution: '2x' }), '2x');
  assert.equal(get('upscaler'), 'auto');
  assert.equal(get('upscaler', { upscaler: 'metalfx' }), 'fsr1');
  assert.equal(get('upscaler', { upscaler: 'bilinear' }), 'bilinear');
  assert.equal(get('antialias', { antialias: 'fxaa' }), 'smaa');
  assert.equal(get('shadows', { shadows: 'high' }), '2x');
  assert.equal(get('show_fps', { show_fps: '1' }), 'top_right');
  assert.equal(get('show_fps', { show_fps: 'bottom_left' }), 'bottom_left');
  assert.equal(get('language', { language: 'zh-TW' }), 'zh-Hant');
  assert.equal(get('language', { language: 'klingon' }), 'auto');
  assert.equal(get('rumble_strength', { rumble_strength: '7' }), '100');
});

test('toggles are atoi and the volume is a clamped float', () => {
  assert.equal(get('fullscreen', { fullscreen: '1' }), '1');
  assert.equal(get('fullscreen', { fullscreen: '2' }), '1');
  assert.equal(get('fullscreen', { fullscreen: '0' }), '0');
  assert.equal(get('reverb', { reverb: '' }), '1');
  assert.equal(get('volume', { volume: '0.35' }), '0.35');
  assert.equal(get('volume', { volume: '9' }), '1.00');
  assert.equal(get('volume', { volume: '-1' }), '0.00');
  assert.equal(get('volume', { volume: 'loud' }), '1.00');
});

test('a saved volume above 100% is shown at the top of the slider and left as it is', () => {
  const ini = { volume: '1.50' };
  assert.equal(get('volume', ini), '1.00');
  assert.deepEqual(ini, { volume: '1.50' });
  assert.equal(row('volume').max, 1);
  assert.equal(row('volume').step, 0.05);
});

test('the defaults are the ones the game uses with no key: original 16:9, background input off', () => {
  assert.equal(get('aspect'), 'original');
  assert.equal(get('aspect', { aspect: 'unlocked' }), 'unlocked');
  assert.equal(get('background_input'), '0');
  assert.equal(get('background_input', { background_input: '1' }), '1');
  const c = readFileSync(new URL('../../runtime/settings.c', import.meta.url), 'utf8');
  assert.match(c, /\.aspect = ASPECT_ORIGINAL,/);
  assert.doesNotMatch(c, /\.bg_input = true/);
});

test('the launcher own preference is read from the prefs, not the ini', () => {
  assert.equal(get('minimise_on_play'), '1');
  assert.equal(get('minimise_on_play', {}, { minimise_on_play: false }), '0');
  assert.equal(get('minimise_on_play', { minimise_on_play: '0' }, {}), '1');
});

test('a row that depends on another is greyed out as in the menu', () => {
  const on = (key, ini) => enabled(row(key), (k) => get(k, ini));
  assert.equal(on('dynamic_min', {}), false);
  assert.equal(on('dynamic_min', { dynamic_resolution: '1' }), true);
  assert.equal(on('resolution', {}), true);
  assert.equal(on('resolution', { dynamic_resolution: '1' }), false);
  assert.equal(on('fps_advanced', {}), false);
  assert.equal(on('fps_advanced', { show_fps: 'top_left' }), true);
  assert.equal(on('rumble_strength', { rumble: '0' }), false);
  assert.equal(on('fullscreen', {}), true);
});

test('what a change stores', () => {
  assert.deepEqual(toWrite(row('fullscreen'), '1'), ['ini', 'fullscreen', '1']);
  assert.deepEqual(toWrite(row('volume'), '0.15000000000000002'), ['ini', 'volume', '0.15']);
  assert.deepEqual(toWrite(row('resolution'), 'match'), ['ini', 'resolution', 'match']);
  assert.deepEqual(toWrite(row('minimise_on_play'), '0'), ['pref', 'minimise_on_play', false]);
});

test('Restore defaults writes every row of the tab, the auto ones as auto', () => {
  const g = restoreWrites('graphics');
  assert.equal(g.length, rowsOf('graphics').length);
  assert.deepEqual(g.find(([, k]) => k === 'resolution'), ['ini', 'resolution', 'auto']);
  assert.deepEqual(g.find(([, k]) => k === 'upscaler'), ['ini', 'upscaler', 'auto']);
  assert.ok(restoreWrites('general').some(([where, k, v]) => where === 'pref' && k === 'minimise_on_play' && v === true));
  assert.deepEqual(restoreWrites('mods'), []);
});
