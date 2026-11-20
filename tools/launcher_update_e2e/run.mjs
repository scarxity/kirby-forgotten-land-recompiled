// The launcher's updater against a local server that plays GitHub's releases API (KFL_UPDATE_API), on copies of an install.
// Usage: node run.mjs <launcher exe of the current version> <the same launcher built with the next version, 0.3.3>
// Both need `--features tauri/custom-protocol` (the page is read over CDP). The launcher windows open on screen.
// E2E_ROOT: scratch folder (default: a folder in the temp folder, deleted first), PY: the Python to zip with.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import os from 'node:os';
import { spawn, spawnSync } from 'node:child_process';

const [EXE_A, EXE_B] = process.argv.slice(2);
const ROOT = process.env.E2E_ROOT || path.join(os.tmpdir(), 'kfl-update-e2e-run');
const PY = process.env.PY || 'python';
const CDP = 9333;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sha = (f) => crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
let failures = 0;
const check = (ok, msg) => {
  console.log(`${ok ? 'PASS' : 'FAIL'} ${msg}`);
  if (!ok) failures++;
};

fs.rmSync(ROOT, { recursive: true, force: true });
fs.mkdirSync(path.join(ROOT, 'shots'), { recursive: true });

function put(file, text) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text);
}

function tree(dir) {
  const out = {};
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else out[path.relative(dir, p).replaceAll('\\', '/')] = sha(p);
    }
  };
  walk(dir);
  return out;
}

const RUNTIME = 'runtime bytes, unchanged between versions';
const PROTECTED = {
  'larecomp.ini': 'fullscreen = 1\n',
  'game-folder.txt': 'D:\\my game',
  'extracted/lasgame.dll': 'built game code',
  'cache/shaders/a.bin': 'shader',
  'mods/m/romfs/a': 'mod',
  'save/slot1.sav': 'my save',
  'game.log': 'log',
  'crash-20260101-000000.log': 'crash',
  'shaders-prepared.txt': 'stamp',
};

function makeZip(name, withBeta) {
  const top = path.join(ROOT, 'zip', name, 'ForgottenLandRecomp');
  fs.copyFileSync(EXE_B, (fs.mkdirSync(top, { recursive: true }), path.join(top, 'ForgottenLandLauncher.exe')));
  put(path.join(top, 'ForgottenLandRecomp.exe'), RUNTIME);
  put(path.join(top, 'zig/zig.exe'), 'new zig');
  put(path.join(top, 'licenses/a.txt'), 'licence ' + name);
  put(path.join(top, 'README.txt'), 'readme ' + name);
  for (const f of Object.keys(PROTECTED)) put(path.join(top, f), 'EVIL, must never be written');
  const zip = path.join(ROOT, 'zip', `${name}.zip`);
  const r = spawnSync(PY, ['-I', path.join(import.meta.dirname, 'mkzip.py'), top, zip]);
  if (r.error || r.status) throw new Error(String(r.error || r.stderr));
  return { zip, digest: sha(zip) };
}

const stable = makeZip('0.3.3');
const beta = makeZip('0.3.4-beta.1');

let server, port, scenario = {}, log = [];
function releases() {
  const asset = (v, z, digest) => ({ name: `ForgottenLandRecomp-${v}-Windows-x64.zip`, browser_download_url: `http://127.0.0.1:${port}/dl/${v}.zip`, digest: `sha256:${digest}` });
  return [
    { tag_name: '0.3.4-beta.1', body: 'Beta notes', draft: false, prerelease: true, assets: [asset('0.3.4-beta.1', beta.zip, beta.digest)] },
    { tag_name: '0.3.3', body: 'What changed in 0.3.3\n- one\n- two', draft: false, prerelease: false, assets: [asset('0.3.3', stable.zip, scenario.badDigest ? 'ab'.repeat(32) : stable.digest)] },
    { tag_name: '0.3.2', body: 'old', draft: false, prerelease: false, assets: [] },
  ];
}
await new Promise((res) => {
  server = http.createServer((req, resp) => {
    log.push(req.url);
    if (req.url.startsWith('/repos/scarxity/kirby-forgotten-land-recompiled/releases')) {
      resp.writeHead(200, { 'content-type': 'application/json' });
      return resp.end(JSON.stringify(releases()));
    }
    const m = req.url.match(/^\/dl\/(.+)\.zip$/);
    if (m) {
      const f = m[1] === '0.3.3' ? stable.zip : beta.zip;
      resp.writeHead(200, { 'content-length': fs.statSync(f).size });
      return fs.createReadStream(f).pipe(resp);
    }
    resp.writeHead(404).end();
  }).listen(0, '127.0.0.1', res);
});
port = server.address().port;

function killTest() {
  spawnSync('powershell', ['-NoProfile', '-Command', `Get-CimInstance Win32_Process | Where-Object { ($_.ExecutablePath -and $_.ExecutablePath.StartsWith('${ROOT}')) -or ($_.CommandLine -and $_.CommandLine.Contains('${ROOT.replaceAll('\\', '\\\\')}')) } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`]);
}

function install(name, prefs) {
  const dir = path.join(ROOT, name);
  fs.mkdirSync(dir, { recursive: true });
  fs.copyFileSync(EXE_A, path.join(dir, 'ForgottenLandLauncher.exe'));
  put(path.join(dir, 'ForgottenLandRecomp.exe'), RUNTIME);
  put(path.join(dir, 'licenses/a.txt'), 'licence old');
  for (const [f, t] of Object.entries(PROTECTED)) put(path.join(dir, f), t);
  put(path.join(dir, 'launcher-prefs.json'), JSON.stringify(prefs));
  return dir;
}

function launch(dir, api = `http://127.0.0.1:${port}`) {
  spawn(path.join(dir, 'ForgottenLandLauncher.exe'), [], {
    cwd: dir,
    detached: true,
    stdio: 'ignore',
    env: { ...process.env, KFL_UPDATE_API: api, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP}`, WEBVIEW2_USER_DATA_FOLDER: path.join(ROOT, 'webview') },
  }).unref();
}

async function evalJs(expression) {
  const targets = await (await fetch(`http://127.0.0.1:${CDP}/json`)).json();
  const t = targets.find((x) => x.type === 'page');
  if (!t) throw new Error('no page');
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((res, rej) => ((ws.onopen = res), (ws.onerror = rej)));
  const ask = (method, params) =>
    new Promise((res) => {
      const id = Math.floor(Math.random() * 1e9);
      ws.onmessage = (m) => {
        const d = JSON.parse(m.data);
        if (d.id === id) res(d.result);
      };
      ws.send(JSON.stringify({ id, method, params }));
    });
  try {
    return { ask, run: async () => (await ask('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result?.value, close: () => ws.close() };
  } catch (e) {
    ws.close();
    throw e;
  }
}
async function js(expression) {
  const s = await evalJs(expression);
  try {
    return await s.run();
  } finally {
    s.close();
  }
}
async function shot(name) {
  const s = await evalJs('0');
  try {
    const r = await s.ask('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(path.join(ROOT, 'shots', name + '.png'), Buffer.from(r.data, 'base64'));
  } finally {
    s.close();
  }
}
const status = () => js("window.__TAURI_INTERNALS__.invoke('status')");
async function until(what, pred, ms = 60000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    try {
      last = await status();
      if (pred(last)) return last;
    } catch {}
    await sleep(400);
  }
  console.log(`timeout waiting for ${what}; last update state: ${JSON.stringify(last?.update)}`);
  return null;
}
const click = (sel) => js(`document.querySelector(${JSON.stringify(sel)}).click()`);
const prefs = (dir) => JSON.parse(fs.readFileSync(path.join(dir, 'launcher-prefs.json'), 'utf8'));

function updatedFilesOk(name, dir) {
  const t = tree(dir);
  check(t['ForgottenLandLauncher.exe'] === sha(EXE_B), `${name}: launcher exe is the new one`);
  check(t['zig/zig.exe'] === crypto.createHash('sha256').update('new zig').digest('hex') && fs.readFileSync(path.join(dir, 'licenses/a.txt'), 'utf8') === 'licence 0.3.3', `${name}: new and changed files written`);
  check(fs.readFileSync(path.join(dir, 'README.txt'), 'utf8') === 'readme 0.3.3', `${name}: README written`);
  for (const [f, text] of Object.entries(PROTECTED)) check(fs.readFileSync(path.join(dir, f), 'utf8') === text, `${name}: ${f} untouched`);
  check(fs.readFileSync(path.join(dir, 'ForgottenLandRecomp.exe'), 'utf8') === RUNTIME, `${name}: unchanged runtime kept`);
}

// 1. Manual: the notice, the channel checkbox, the Update button
console.log('\n== 1. manual update through the page');
killTest();
let dir = install('i1', { minimise: true });
const runtimeTime = fs.statSync(path.join(dir, 'ForgottenLandRecomp.exe')).mtimeMs;
launch(dir);
let s = await until('available 0.3.3', (x) => x.update.state === 'available');
check(s?.update.version === '0.3.3' && s.update.notes.includes('one') && s.version === '0.3.2', '1: Version 0.3.3 offered on the stable channel with its notes (beta.1 not offered)');
check(s?.auto_update === false && s?.stable_only === true, '1: defaults are auto off, stable only on');
await shot('1-home-available');
check((await js("document.querySelector('.update h2')?.textContent")) === 'Version 0.3.3 is available' && (await js("!!document.querySelector('.update details')")), '1: Home shows the version line and What is new');
await click('.nav-item[data-page=about]');
await sleep(700);
await shot('1-about');
await click('[data-pref=stable_only]');
s = await until('beta offered', (x) => x.update.state === 'available' && x.update.version === '0.3.4-beta.1');
check(!!s && s.stable_only === false && prefs(dir).stable_only === false && prefs(dir).minimise === true, '1: unticking stable offers the beta at once, saved in launcher-prefs.json next to the other key');
await shot('1-about-beta');
await click('[data-pref=stable_only]');
s = await until('stable again', (x) => x.update.state === 'available' && x.update.version === '0.3.3');
check(!!s, '1: ticking it again goes back to 0.3.3');
await click('.nav-item[data-page=home]');
await sleep(600);
const logBefore = log.filter((u) => u.startsWith('/dl/')).length;
await click('[data-act=update]');
s = await until('restarted as 0.3.3', (x) => x.version === '0.3.3', 90000);
check(!!s, '1: the launcher restarted as 0.3.3');
await sleep(1500);
updatedFilesOk('1', dir);
check(fs.statSync(path.join(dir, 'ForgottenLandRecomp.exe')).mtimeMs === runtimeTime, '1: an unchanged runtime keeps its time stamp (no needless rebuild or shader preparation)');
check(log.filter((u) => u.startsWith('/dl/')).length - logBefore === 1, '1: one download');
s = await until('up to date and cleaned', (x) => x.update.state === 'up_to_date' && !fs.existsSync(path.join(dir, '.update')));
check(!!s, '1: the new launcher finds nothing newer and deleted .update');
check(prefs(dir).minimise === true && prefs(dir).stable_only === true, '1: preferences file survived');
await shot('1-after');
killTest();
await sleep(1000);

// 2. Bad digest, update automatically
console.log('\n== 2. bad digest aborts');
scenario = { badDigest: true };
dir = install('i2', { auto_update: true });
const before = tree(dir);
launch(dir);
s = await until('failed', (x) => x.update.state === 'failed');
check(s?.update.msg?.includes('checksum') && s.version === '0.3.2', `2: the update failed with "${s?.update.msg}" and the launcher stayed on 0.3.2`);
await shot('2-failed');
check((await js("document.querySelector('.update .bad')?.textContent"))?.includes('checksum'), '2: Home shows the failure');
check(JSON.stringify(tree(dir)) === JSON.stringify(before), '2: every file in the install is byte for byte as before');
check(!fs.existsSync(path.join(dir, '.update')), '2: the download was deleted');
killTest();
await sleep(1000);

// 3. Update automatically
console.log('\n== 3. update automatically');
scenario = {};
dir = install('i3', { auto_update: true });
launch(dir);
s = await until('restarted as 0.3.3', (x) => x.version === '0.3.3', 90000);
check(!!s, '3: found, installed and restarted by itself');
await sleep(1500);
updatedFilesOk('3', dir);
check(prefs(dir).auto_update === true, '3: the setting is still on');
killTest();
await sleep(1000);

// 4. No internet
console.log('\n== 4. no internet');
dir = install('i4', {});
launch(dir, 'http://127.0.0.1:1');
s = await until('idle', (x) => x.update.state === 'idle', 20000);
await sleep(3000);
s = await status();
check(s.update.state === 'idle', '4: silent at start (state idle, no error)');
await click('.nav-item[data-page=about]');
await sleep(500);
await click('[data-act=check-update]');
s = await until('failed', (x) => x.update.state === 'failed', 20000);
check(s?.update.msg?.startsWith('Could not check for updates'), `4: the About button says so: ${s?.update.msg}`);
await shot('4-no-internet');
killTest();

server.close();
console.log(failures ? `\n${failures} FAILED` : '\nALL PASSED');
process.exit(failures ? 1 : 0);
