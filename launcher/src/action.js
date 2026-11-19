export const BUSY_PHASES = ['runtime', 'files', 'translate', 'compile', 'shaders'];

// The one thing the big button does next, from the backend status. Pure, so it can be tested without a window.
export function primaryAction(s) {
  if (!s) return { label: 'Loading', act: null, busy: true };
  if (s.running) return { label: 'Running', act: null, busy: true };
  if (BUSY_PHASES.includes(s.phase)) return { label: 'Setting up', act: 'go-setup', busy: true };
  if (s.work_error) return { label: 'Set up', act: 'go-setup', detail: s.work_error };
  if (s.rebuild && !s.tools_missing.length) {
    return { label: 'Rebuild game code', act: 'rebuild', detail: s.phase === 'failed' ? s.error : REBUILD_NOTE };
  }
  if (!s.game_dir) return { label: 'Select game folder', act: 'pick' };
  if (s.game_error) return { label: 'Select game folder', act: 'pick', detail: s.game_error };
  if (s.tools_missing.length && !s.built) return { label: 'Set up', act: 'go-setup', detail: toolsMissing(s) };
  if (!s.built) return { label: 'Set up', act: 'setup', detail: s.phase === 'failed' ? s.error : '' };
  if (s.shaders_ready === false) return { label: 'Play', act: 'play', detail: SHADERS_NOTE };
  return { label: 'Play', act: 'play' };
}

export const SHADERS_NOTE = 'Play prepares the shaders first, about 8 minutes, so nothing is compiled while you play. It happens again only after a graphics driver or mod change.';

export const REBUILD_NOTE = 'The game code was built by another version of this program. Build it again to play. It takes about 7 minutes on a 16-thread PC.';

export function toolsMissing(s) {
  const list = s.tools_missing.join(', ');
  return s.packaged
    ? `Missing: ${list}. Unzip the whole release folder and start the launcher from it. If an antivirus removed a file, restore it and allow this folder in the antivirus.`
    : `Missing: ${list}. See the README for the install commands.`;
}

export const PHASE_LABEL = {
  runtime: 'Building the runtime',
  files: 'Preparing the game files',
  translate: 'Translating the game code',
  compile: 'Compiling the game code',
  shaders: 'Preparing shaders',
};

// Mods are read when the game starts and the mods folder may be in use by the setup: leave them alone meanwhile. The
// settings follow the same rule, as the game rewrites larecomp.ini while it runs.
export function modsLocked(s) {
  return s.running || BUSY_PHASES.includes(s.phase);
}
