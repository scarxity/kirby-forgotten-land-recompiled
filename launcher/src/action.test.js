import assert from 'node:assert/strict';
import { test } from 'node:test';
import { BUSY_PHASES, PHASE_LABEL, REBUILD_NOTE, SHADERS_NOTE, modsLocked, primaryAction, toolsMissing } from './action.js';

const base = { running: false, phase: 'idle', game_dir: 'D:\dump', game_error: null, tools_missing: [], built: true, error: '' };

test('primary action follows the setup state', () => {
  assert.equal(primaryAction(null).busy, true);
  assert.equal(primaryAction({ ...base, game_dir: null }).act, 'pick');
  assert.equal(primaryAction({ ...base, game_error: 'another game' }).act, 'pick');
  assert.equal(primaryAction({ ...base, built: false }).act, 'setup');
  assert.equal(primaryAction({ ...base, built: false, tools_missing: ['MSYS2 clang'] }).act, 'go-setup');
  assert.equal(primaryAction({ ...base, built: false, phase: 'compile' }).busy, true);
  assert.equal(primaryAction({ ...base, built: false, phase: 'failed', error: 'boom' }).detail, 'boom');
  assert.equal(primaryAction({ ...base, built: false, phase: 'files' }).busy, true);
  assert.equal(primaryAction(base).act, 'play');
  assert.equal(primaryAction({ ...base, running: true }).act, null);
});

test('Play says it prepares the shaders first when they are not prepared for this driver', () => {
  assert.deepEqual(primaryAction({ ...base, shaders_ready: false }), { label: 'Play', act: 'play', detail: SHADERS_NOTE });
  assert.equal(primaryAction({ ...base, shaders_ready: true }).detail, undefined);
  assert.equal(primaryAction({ ...base, phase: 'shaders' }).busy, true);
});

test('game code built by another version is offered a rebuild, not Play', () => {
  const stale = { ...base, built: false, rebuild: true, packaged: true };
  assert.equal(primaryAction(stale).act, 'rebuild');
  assert.equal(primaryAction(stale).detail, REBUILD_NOTE);
  assert.equal(primaryAction({ ...stale, game_dir: null }).act, 'rebuild', 'the dump is not needed for a rebuild');
  assert.equal(primaryAction({ ...stale, phase: 'failed', error: 'boom' }).detail, 'boom');
  assert.equal(primaryAction({ ...stale, tools_missing: ['the compiler (zig)'] }).act, 'go-setup');
  assert.equal(primaryAction({ ...stale, phase: 'compile' }).busy, true);
});

test('a folder the build cannot work in is said up front', () => {
  const s = primaryAction({ ...base, built: false, work_error: 'The path is too long.' });
  assert.deepEqual([s.act, s.detail], ['go-setup', 'The path is too long.']);
});

test('missing build tools say what to do', () => {
  const packaged = toolsMissing({ tools_missing: ['the compiler (zig)'], packaged: true });
  assert.match(packaged, /antivirus/);
  assert.doesNotMatch(packaged, /README/);
  assert.match(toolsMissing({ tools_missing: ['MSYS2 make'], packaged: false }), /README/);
});

test('every busy phase has a label', () => {
  for (const p of BUSY_PHASES) assert.ok(PHASE_LABEL[p], p);
  assert.equal(PHASE_LABEL.files, 'Preparing the game files');
  for (const p of BUSY_PHASES) assert.doesNotMatch(PHASE_LABEL[p], /image/i, `${p}: the runtime builds no image`);
});

test('mods are left alone while the game runs or a build runs', () => {
  assert.equal(modsLocked(base), false);
  assert.equal(modsLocked({ ...base, phase: 'failed' }), false);
  assert.equal(modsLocked({ ...base, running: true }), true);
  for (const p of BUSY_PHASES) assert.equal(modsLocked({ ...base, phase: p }), true, p);
});
