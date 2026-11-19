export const TABS = [
  ['display', 'Display'],
  ['graphics', 'Graphics'],
  ['audio', 'Audio'],
  ['controls', 'Controls'],
  ['general', 'General'],
  ['mods', 'Mods'],
  ['game', 'Game'],
];

const offCorners = [['off', 'Off'], ['top_left', 'Top left'], ['top_right', 'Top right'], ['bottom_left', 'Bottom left'], ['bottom_right', 'Bottom right']];

// One entry per setting, in the order of the in-game menu (runtime/ui/menu.mm). `key` and the values are what
// runtime/settings.c reads from larecomp.ini, as strings. `read` maps a value the game still accepts from an older
// file onto an option. `needs` greys the row out while another row has the wrong value, as the menu does. `pref` rows
// are the launcher's own (launcher-prefs.json), not the game's.
export const ROWS = [
  { key: 'aspect', label: 'Aspect ratio', tab: 'display', kind: 'choice', options: [['unlocked', 'Unlocked'], ['original', 'Original 16:9']], default: 'unlocked' },
  { key: 'fullscreen', label: 'Fullscreen', tab: 'display', kind: 'toggle', default: '0' },
  { key: 'show_fps', label: 'Show FPS', tab: 'display', kind: 'choice', options: offCorners, default: 'off', read: (raw) => (parseInt(raw, 10) ? 'top_right' : 'off') },
  { key: 'fps_advanced', label: 'Show advanced information', tab: 'display', kind: 'toggle', default: '0', needs: { key: 'show_fps', not: 'off' } },
  { key: 'remember_window', label: 'Remember window size', tab: 'display', kind: 'toggle', default: '0' },

  {
    key: 'resolution', label: 'Resolution', tab: 'graphics', section: 'Rendering', kind: 'choice',
    options: [['25%', '25%'], ['50%', '50%'], ['75%', '75%'], ['match', 'Match display'], ['1x', '1x'], ['2x', '2x'], ['3x', '3x'], ['4x', '4x']],
    // "auto" is the GPU's default, which is Match display on every GPU today; 5x and up were removed
    default: 'auto', needs: { key: 'dynamic_resolution', is: '0' },
    read: (raw) => {
      const n = parseInt(raw, 10);
      return raw === '67%' ? '75%' : n > 4 ? '4x' : n >= 1 ? `${n}x` : 'match';
    },
  },
  { key: 'dynamic_resolution', label: 'Dynamic resolution', tab: 'graphics', section: 'Rendering', kind: 'toggle', default: '0' },
  { key: 'dynamic_min', label: 'Minimum resolution', tab: 'graphics', section: 'Rendering', kind: 'choice', options: [['360', '360p'], ['540', '540p'], ['720', '720p']], default: '540', needs: { key: 'dynamic_resolution', is: '1' } },
  // "auto": FSR 1 on a discrete GPU, Bilinear on an integrated one. The launcher cannot tell which, so it says so.
  { key: 'upscaler', label: 'Upscaler', tab: 'graphics', section: 'Rendering', kind: 'choice', options: [['auto', 'Automatic for this GPU'], ['bilinear', 'Bilinear'], ['fsr1', 'AMD FSR 1']], default: 'auto', read: () => 'fsr1' },
  { key: 'antialias', label: 'Anti-aliasing', tab: 'graphics', section: 'Rendering', kind: 'choice', options: [['off', 'Off'], ['smaa', 'SMAA']], default: 'off', read: () => 'smaa' },
  { key: 'ambient_occlusion', label: 'Ambient occlusion', tab: 'graphics', section: 'Rendering', kind: 'choice', options: [['switch', 'Default'], ['high', 'High'], ['off', 'Off']], default: 'switch' },
  { key: 'shadows', label: 'Shadows', tab: 'graphics', section: 'Rendering', kind: 'choice', options: [['off', 'Off'], ['0.5x', '0.5x'], ['default', 'Default'], ['2x', '2x'], ['3x', '3x'], ['4x', '4x']], default: 'default', read: (raw) => (raw === 'high' ? '2x' : 'default') },
  { key: 'frame_interp', label: 'Frame Generation', tab: 'graphics', section: 'Experimental', kind: 'toggle', default: '0' },

  { key: 'volume', label: 'Volume', tab: 'audio', section: 'Volume', kind: 'slider', min: 0, max: 2, step: 0.05, default: '1.00' },
  { key: 'audio_output', label: 'Output', tab: 'audio', section: 'Volume', kind: 'choice', options: [['stereo', 'Stereo'], ['5.1', '5.1']], default: 'stereo' },
  { key: 'movie_audio', label: 'Movie audio', tab: 'audio', section: 'Volume', kind: 'toggle', default: '1' },
  { key: 'reverb', label: 'Enable reverb', tab: 'audio', section: 'Effects', kind: 'toggle', default: '1' },

  { key: 'rumble', label: 'Rumble', tab: 'controls', kind: 'toggle', default: '1' },
  { key: 'rumble_strength', label: 'Rumble intensity', tab: 'controls', kind: 'choice', options: [['25', '25%'], ['50', '50%'], ['75', '75%'], ['100', '100%']], default: '100', needs: { key: 'rumble', is: '1' } },
  { key: 'prompts', label: 'Button icons', tab: 'controls', kind: 'choice', options: [['auto', 'Auto'], ['nintendo', 'Nintendo'], ['xbox', 'Xbox'], ['playstation', 'PlayStation']], default: 'auto' },
  { key: 'face_buttons_by_label', label: 'Button layout', tab: 'controls', kind: 'choice', options: [['0', 'Switch'], ['1', 'Xbox / PlayStation']], default: '0' },
  // On except on macOS, where it needs a permission (runtime/settings.c)
  { key: 'background_input', label: 'Allow background inputs', tab: 'controls', kind: 'toggle', default: '1' },

  {
    key: 'language', label: 'Language', tab: 'general', kind: 'choice', default: 'auto',
    options: [['auto', 'Auto'], ['ja', '日本語'], ['en-US', 'English'], ['es', 'Español'], ['fr', 'Français'], ['de', 'Deutsch'], ['it', 'Italiano'], ['nl', 'Nederlands'], ['ru', 'Русский'], ['ko', '한국어'], ['zh-Hans', '简体中文'], ['zh-Hant', '繁體中文'], ['pt-BR', 'Português (Brasil)']],
    read: (raw) => ({ 'zh-CN': 'zh-Hans', 'zh-TW': 'zh-Hant' })[raw] ?? 'auto',
  },
  { key: 'pause_unfocused', label: 'Pause on focus lost', tab: 'general', kind: 'toggle', default: '0' },
  { key: 'minimise_on_play', label: 'Minimise launcher while playing', tab: 'general', kind: 'toggle', default: '1', pref: true },
];

export const rowsOf = (tab) => ROWS.filter((r) => r.tab === tab);

// The row's value as the game would read it: a missing, empty or unknown entry is the default.
export function valueOf(row, ini, prefs) {
  if (row.pref) return typeof prefs?.[row.key] === 'boolean' ? (prefs[row.key] ? '1' : '0') : row.default;
  const raw = ini[row.key] || row.default;
  if (row.kind === 'toggle') return parseInt(raw, 10) ? '1' : '0';
  if (row.kind === 'slider') {
    const n = Number(raw);
    return Number.isFinite(n) ? Math.min(row.max, Math.max(row.min, n)).toFixed(2) : row.default;
  }
  if (row.options.some(([v]) => v === raw)) return raw;
  return row.read?.(raw) ?? row.default;
}
export function enabled(row, value) {
  const n = row.needs;
  if (!n) return true;
  return 'is' in n ? value(n.key) === n.is : value(n.key) !== n.not;
}

// What to store for a value the page picked: [where, key, stored value]
export function toWrite(row, value) {
  if (row.pref) return ['pref', row.key, value === '1'];
  return ['ini', row.key, row.kind === 'slider' ? Number(value).toFixed(2) : value];
}

export const restoreWrites = (tab) => rowsOf(tab).map((r) => toWrite(r, r.default));
