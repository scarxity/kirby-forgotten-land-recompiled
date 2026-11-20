import './style.css';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { BUSY_PHASES, PHASE_LABEL, REBUILD_NOTE, modsLocked, primaryAction, toolsMissing, updateNotice } from './action.js';
import { ROWS, TABS, enabled, restoreWrites, rowsOf, toWrite, valueOf } from './settings.js';

const PAGES = [
  ['home', 'Home'],
  ['setup', 'Set up'],
  ['settings', 'Settings'],
  ['about', 'About'],
];
const PACKAGED_BUILD = 'Copies your game files into this folder (about 6 GB, or nothing extra when Windows can hard-link them on the same drive), then translates the game code and compiles it for this PC. The first build takes a long time.';

const $ = (sel) => document.querySelector(sel);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
let status = null;
let page = 'home';
let drawn = '';
let settingsTab = 'display';
// the tab whose Restore defaults waits for its confirmation
let confirming = null;

function toast(msg) {
  const el = $('#toast');
  el.textContent = msg;
  el.classList.add('show');
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => el.classList.remove('show'), 6000);
}

async function call(cmd, args) {
  let result;
  try {
    result = await invoke(cmd, args);
  } catch (e) {
    toast(String(e));
  }
  await refresh();
  return result;
}

// settings.js's [where, key, value] list: the game's ini keys in one write, the launcher's own prefs in another
async function write(list) {
  const entries = list.filter(([where]) => where === 'ini').map(([, key, value]) => [key, value]);
  const values = Object.fromEntries(list.filter(([where]) => where === 'pref').map(([, key, value]) => [key, value]));
  if (entries.length) await call('set_settings', { entries });
  if (Object.keys(values).length) await call('set_prefs', { values });
}

const actions = {
  async pick() {
    const dir = await open({ directory: true, title: 'Folder with your extracted exefs and romfs' });
    if (dir) await call('set_game_dir', { path: dir });
  },
  setup: () => call('start_setup'),
  rebuild: () => call('start_setup', { codeOnly: true }),
  cancel: () => call('cancel_setup'),
  play: () => call('play'),
  prepare: () => call('prepare_shaders'),
  'go-setup': () => go('setup'),
  'open-work': () => call('open_path', { which: 'work' }),
  'open-mods': () => call('open_path', { which: 'mods' }),
  'mod-up': ({ mod }) => call('move_mod', { name: mod, up: true }),
  'mod-down': ({ mod }) => call('move_mod', { name: mod, up: false }),
  'open-saves': () => call('open_path', { which: 'saves' }),
  async 'backup-saves'() {
    const dir = await call('backup_saves');
    if (dir) toast(`Saves backed up to ${dir}`);
  },
  async 'build-again'() {
    await actions.rebuild();
    go('setup');
  },
  'restore-ask': ({ tab }) => {
    confirming = tab;
    draw();
  },
  'restore-no': () => {
    confirming = null;
    draw();
  },
  async 'restore-yes'({ tab }) {
    confirming = null;
    await write(restoreWrites(tab));
  },
  'open-setup-log': () => call('open_path', { which: 'setup_log' }),
  'open-game-log': () => call('open_path', { which: 'game_log' }),
  'open-crash-log': () => call('open_path', { which: 'crash_log' }),
  'dismiss-crash': () => call('dismiss_crash'),
  update: () => call('start_update'),
  'cancel-update': () => call('cancel_update'),
  'check-update': () => call('check_update'),
};

const busy = (s) => BUSY_PHASES.includes(s.phase);
const buildBlocked = (s) => busy(s) || s.work_error || s.tools_missing.length || (!s.rebuild && (!s.game_dir || s.game_error));

function progress(s) {
  if (!busy(s)) return '';
  const pct = s.progress == null ? null : Math.min(100, Math.round(s.progress * 100));
  return `<div class="task glass">
    <span class="task-msg">${esc(PHASE_LABEL[s.phase])}${s.detail ? ` ${esc(s.detail)}` : ''}</span>
    <span class="task-pct">${pct == null ? '' : `${pct}%`}</span>
    <progress max="100" ${pct == null ? '' : `value="${pct}"`}></progress>
    <button class="btn sm" data-act="cancel">Cancel</button>
  </div>`;
}

// The update line for Home (only what there is to act on) or About (every state)
function updateBlock(s, onHome) {
  const n = updateNotice(s);
  if (!n || (onHome && ['checking', 'current', 'managed'].includes(n.kind))) return '';
  if (n.kind === 'busy') {
    const pct = n.progress == null ? null : Math.min(100, Math.round(n.progress * 100));
    return `<h2>${esc(n.title)}</h2>
      <progress max="100" ${pct == null ? '' : `value="${pct}"`}></progress>
      ${n.cancel ? '<div class="row"><button class="btn sm" data-act="cancel-update">Cancel</button></div>' : ''}`;
  }
  if (n.kind !== 'available') {
    const failed = n.kind === 'failed';
    return `<p class="${failed ? 'bad' : ''}" role="${failed ? 'alert' : 'status'}">${esc(n.title)}</p>`;
  }
  return `<h2>${esc(n.title)}</h2>
    <div class="row"><button class="btn primary" data-act="update" ${n.canUpdate ? '' : 'disabled'}>Update</button></div>
    ${n.hint ? `<p class="fine">${esc(n.hint)}</p>` : ''}
    ${n.notes ? `<details><summary>What's new</summary><pre class="notes">${esc(n.notes)}</pre></details>` : ''}`;
}

function crashCard(c) {
  if (!c) return '';
  return `<section class="card glass crash" role="alert">
    <h2>The game stopped unexpectedly</h2>
    <p class="bad">${esc(c.reason)}</p>
    <p class="fine">Saved as <code>${esc(c.log)}</code></p>
    <div class="row">
      <button class="btn" data-act="open-crash-log">Open crash log</button>
      <button class="btn quiet" data-act="open-work">Open work folder</button>
      <button class="btn quiet" data-act="dismiss-crash">Dismiss</button>
    </div>
  </section>`;
}

function modsCard(s) {
  const locked = modsLocked(s) ? 'disabled' : '';
  const row = (m, i) => `<div class="field mod ${m.on ? '' : 'off'}">
      <span class="mod-name">${esc(m.name)}</span>
      <button class="btn sm quiet" data-act="mod-up" data-mod="${esc(m.name)}" aria-label="Move ${esc(m.name)} up" ${locked || i === 0 ? 'disabled' : ''}>&#9650;</button>
      <button class="btn sm quiet" data-act="mod-down" data-mod="${esc(m.name)}" aria-label="Move ${esc(m.name)} down" ${locked || i === s.mods.length - 1 ? 'disabled' : ''}>&#9660;</button>
      <input type="checkbox" data-mod="${esc(m.name)}" aria-label="${esc(m.name)} on" ${m.on ? 'checked' : ''} ${locked} />
    </div>`;
  return `<section class="card glass">
    <h2>Mods</h2>
    ${s.mods.length
      ? `<p>A mod higher in the list wins when two change the same file.</p>${s.mods.map(row).join('')}`
      : '<p>No mods are installed. To install one, put the folder of the mod, the one that holds <code>romfs</code>, into the mods folder.</p>'}
    ${locked ? '<p class="fine">Mods cannot be changed while the game or a build is running.</p>' : ''}
    <div class="row mods-row"><button class="btn" data-act="open-mods">Open mods folder</button></div>
    <p class="fine">Mods are made by players and are not part of this project.</p>
  </section>`;
}

const pct = (v) => `${Math.round(v * 100)}%`;

function settingRow(r, value, off) {
  const v = value(r.key);
  const key = `data-key="${r.key}" ${off ? 'disabled' : ''}`;
  const control = {
    toggle: () => `<input type="checkbox" ${key} ${v === '1' ? 'checked' : ''} />`,
    choice: () => `<select ${key}>${r.options.map(([val, name]) => `<option value="${esc(val)}" ${val === v ? 'selected' : ''}>${esc(name)}</option>`).join('')}</select>`,
    slider: () => `<input type="range" min="${r.min}" max="${r.max}" step="${r.step}" ${key} value="${esc(v)}" /><output>${pct(v)}</output>`,
  }[r.kind]();
  return `<label class="field ${off ? 'off' : ''}"><span>${esc(r.label)}</span>${control}</label>`;
}

// the tab's rows, a card per section of the in-game menu
function rowsCards(s) {
  const ini = Object.fromEntries(s.settings);
  const value = (key) => valueOf(ROWS.find((r) => r.key === key), ini, s.prefs);
  const locked = modsLocked(s);
  const sections = new Map();
  for (const r of rowsOf(settingsTab)) sections.set(r.section ?? '', [...(sections.get(r.section ?? '') ?? []), r]);
  const name = TABS.find(([id]) => id === settingsTab)[1];
  return `${settingsTab === 'controls' ? `<section class="card glass">
      <h2>Inputs</h2>
      <p>Keys and buttons are rebound in the game's own menu, where you can press them. Open it while playing with F1 or Esc (on a controller, Minus and Plus together), then choose Controls and Configure inputs. The bindings you have are left as they are.</p>
    </section>` : ''}
    ${[...sections].map(([section, rows]) => `<section class="card glass">
      ${section ? `<h2>${esc(section)}</h2>` : ''}
      ${rows.map((r) => settingRow(r, value, locked || !enabled(r, value))).join('')}
    </section>`).join('')}
    ${locked ? '<p class="fine">Settings cannot be changed while the game or a build is running.</p>' : ''}
    ${confirming === settingsTab
      ? `<div class="row confirm" role="alert"><span>Restore the ${name} settings to their defaults?</span>
          <button class="btn sm" data-act="restore-yes" data-tab="${settingsTab}">Restore</button>
          <button class="btn sm quiet" data-act="restore-no">Cancel</button></div>`
      : `<div class="row"><button class="btn quiet" data-act="restore-ask" data-tab="${settingsTab}" ${locked ? 'disabled' : ''}>Restore defaults</button></div>`}`;
}

function gameCard(s) {
  const card = (title, body) => `<section class="card glass"><h2>${title}</h2>${body}</section>`;
  return `${card('Game folder', `
      <p>${s.game_dir ? `<code>${esc(s.game_dir)}</code>` : 'No folder picked yet.'}</p>
      ${s.game_error ? `<p class="bad" role="alert">${esc(s.game_error)}</p>` : ''}
      <button class="btn" data-act="pick" ${busy(s) || s.running ? 'disabled' : ''}>Change</button>`)}
    ${card('Game code', `
      <p>Translates and compiles the game code again, for example after the build failed or a mod needs it.</p>
      <button class="btn" data-act="build-again" ${buildBlocked(s) || s.running ? 'disabled' : ''}>Build again</button>`)}
    ${card('Logs', `<div class="row">
        <button class="btn" data-act="open-game-log">Open game log</button>
        <button class="btn" data-act="open-setup-log">Open setup log</button>
        <button class="btn" data-act="open-crash-log" ${s.crash ? '' : 'disabled'}>Open crash log</button>
      </div>
      ${s.crash ? '' : '<p class="fine">The crash log is there after the game stops unexpectedly.</p>'}`)}
    ${card('Saves', `<div class="row">
        <button class="btn" data-act="open-saves">Open saves folder</button>
        <button class="btn" data-act="backup-saves" ${s.running ? 'disabled' : ''}>Back up saves now</button>
      </div>
      <p class="fine">Backups go to the <code>save-backups</code> folder next to the program. The last 5 are kept.</p>`)}`;
}

const views = {
  home(s) {
    const a = primaryAction(s);
    return `<div class="page home">
      <h1 class="wordmark"><span>Forgotten Land</span><small>Recompiled</small></h1>
      <div class="home-actions">
        <button class="btn primary big" data-act="${a.act ?? ''}" ${a.act ? '' : 'disabled'} aria-busy="${!!a.busy}">${esc(a.label)}</button>
      </div>
      ${a.detail ? `<p class="detail glass" role="alert">${esc(a.detail)}</p>` : ''}
      ${s.phase === 'failed' ? '<div class="home-actions"><button class="btn quiet" data-act="open-setup-log">Open setup log</button></div>' : ''}
      ${updateBlock(s, true) ? `<section class="card glass update">${updateBlock(s, true)}</section>` : ''}
      ${crashCard(s.crash)}
      ${progress(s)}
      <p class="fine">Bring your own copy of the game. Nothing here contains or shares game files.</p>
    </div>`;
  },
  setup(s) {
    const step = (n, title, ok, body) => `<section class="card glass">
      <h2><span class="step ${ok ? 'ok' : ''}">${ok ? '&#10003;' : n}</span>${title}</h2>${body}</section>`;
    return `<div class="page">
      <h1>Set up</h1>
      ${step(1, 'Game files', s.game_dir && !s.game_error, `
        <p>${s.game_dir ? `<code>${esc(s.game_dir)}</code>` : 'Pick the folder that holds the <code>exefs</code> and <code>romfs</code> you extracted from your own copy.'}</p>
        ${s.game_error ? `<p class="bad" role="alert">${esc(s.game_error)}</p>` : ''}
        <button class="btn" data-act="pick" ${busy(s) ? 'disabled' : ''}>Select game folder</button>`)}
      ${step(2, 'Build tools', !s.tools_missing.length, s.tools_missing.length
        ? `<p class="bad" role="alert">${esc(toolsMissing(s))}</p>`
        : `<p>${s.packaged ? 'The bundled game code generator and compiler are in this folder.' : 'MSYS2 clang, make and the Python environment are installed.'}</p>`)}
      ${step(3, 'Game code', s.built, `
        <p>${s.built ? 'The game code is built for this PC.' : s.rebuild ? REBUILD_NOTE : s.packaged ? PACKAGED_BUILD : 'Builds the runtime, checks your game files, then translates the game code from them and compiles it for this PC. About 7 minutes on a 16-thread CPU.'}</p>
        ${s.work_error ? `<p class="bad" role="alert">${esc(s.work_error)}</p>` : ''}
        ${s.phase === 'failed' ? `<p class="bad" role="alert">${esc(s.error)}</p>` : ''}
        ${progress(s)}
        <div class="row">
          <button class="btn ${s.built ? '' : 'primary'}" data-act="${s.rebuild ? 'rebuild' : 'setup'}" ${buildBlocked(s) ? 'disabled' : ''}>${s.built || s.rebuild ? 'Rebuild' : 'Build'}</button>
          <button class="btn quiet" data-act="open-setup-log">Open setup log</button>
        </div>`)}
    </div>`;
  },
  settings(s) {
    const tabs = TABS.map(([id, name]) => `<button class="tab" data-tab="${id}" aria-pressed="${id === settingsTab}">${name}</button>`).join('');
    const body = { mods: modsCard, game: gameCard }[settingsTab] ?? rowsCards;
    return `<div class="page">
      <h1>Settings</h1>
      <div class="tabs">${tabs}</div>
      ${body(s)}
      <p class="fine">Changes apply the next time the game starts.</p>
    </div>`;
  },
  about(s) {
    return `<div class="page">
      <h1>About</h1>
      <section class="card glass">
        <p>A static recompilation of the game's code into a native program, built on your PC from your own copy. It is not an emulator and it ships no game files, keys or dumping tools.</p>
        <p>Built on open-source recompilation projects (MIT): their licences are in the licenses folder. Not affiliated with Nintendo or HAL Laboratory.</p>
        <p>Work folder: <code>${esc(s.work)}</code></p>
        <div class="row">
          <button class="btn" data-act="open-work">Open work folder</button>
          <button class="btn quiet" data-act="open-game-log">Open game log</button>
        </div>
      </section>
      <section class="card glass update">
        <h2>Version ${esc(s.version)}</h2>
        ${s.managed ? updateBlock(s, false) : s.packaged ? `${updateBlock(s, false)}
        <div class="row"><button class="btn" data-act="check-update" ${['checking', 'downloading', 'installing'].includes(s.update.state) ? 'disabled' : ''}>Check for updates</button></div>
        <label class="field"><span>Update automatically</span>
          <input type="checkbox" data-pref="auto_update" ${s.auto_update ? 'checked' : ''} /></label>
        <label class="field"><span>Only update to stable versions</span>
          <input type="checkbox" data-pref="stable_only" ${s.stable_only ? 'checked' : ''} /></label>
        <p class="fine">Untick to also get beta versions: new fixes early, possibly less tested.</p>` : '<p class="fine">Updates are for the release folder.</p>'}
      </section>
    </div>`;
  },
};

function draw() {
  if (!status) return;
  const html = views[page](status);
  // status arrives twice a second: rewrite only on change, so focus and open selects survive
  if (html === drawn) return;
  drawn = html;
  $('#view').innerHTML = html;
}

// built once: rebuilding it on every page change would drop the keyboard focus
$('#nav').innerHTML = `<div class="brand">Forgotten Land<small>Recompiled</small></div>${PAGES.map(
  ([id, name]) => `<button class="nav-item" data-page="${id}">${name}</button>`,
).join('')}`;

function go(to) {
  page = to;
  document.body.dataset.page = to;
  for (const b of document.querySelectorAll('.nav-item')) {
    if (b.dataset.page === to) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  }
  drawn = '';
  draw();
}

async function refresh() {
  const wasRunning = status?.running;
  try {
    status = await invoke('status');
  } catch (e) {
    toast(String(e));
  }
  if (status?.running && !wasRunning && valueOf(ROWS.find((r) => r.key === 'minimise_on_play'), {}, status.prefs) === '1') getCurrentWindow().minimize();
  draw();
}

document.addEventListener('click', (e) => {
  const nav = e.target.closest('.nav-item');
  if (nav) return go(nav.dataset.page);
  const tab = e.target.closest('.tab');
  if (tab) {
    settingsTab = tab.dataset.tab;
    confirming = null;
    draw();
    return $(`.tab[data-tab="${settingsTab}"]`).focus();
  }
  const el = e.target.closest('[data-act]');
  if (el && actions[el.dataset.act]) actions[el.dataset.act](el.dataset);
});
document.addEventListener('change', (e) => {
  if (e.target.dataset?.pref) return call('set_pref', { key: e.target.dataset.pref, on: e.target.checked });
  if (e.target.dataset?.mod) return call('set_mod', { name: e.target.dataset.mod, on: e.target.checked });
  const key = e.target.dataset?.key;
  if (!key) return;
  const value = e.target.type === 'checkbox' ? (e.target.checked ? '1' : '0') : e.target.value;
  write([toWrite(ROWS.find((r) => r.key === key), value)]);
});
document.addEventListener('input', (e) => {
  if (e.target.type === 'range') e.target.nextElementSibling.textContent = pct(e.target.value);
});
$('#win-min').addEventListener('click', () => getCurrentWindow().minimize());
$('#win-close').addEventListener('click', () => getCurrentWindow().close());

go('home');
refresh();
setInterval(refresh, 500);
