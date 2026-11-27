"""Native settings regression checks; no game files or player settings are used."""
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory() as directory:
    directory = Path(directory)
    exe = directory / ('settings-test.exe' if os.name == 'nt' else 'settings-test')
    subprocess.run(shlex.split(os.environ.get('CC', 'clang')) +
                   ['-std=gnu11', '-Iruntime', 'runtime/settings.c', 'tools/test_settings.c', '-o', str(exe), '-lm'] +
                   (['-framework', 'CoreFoundation'] if sys.platform == 'darwin' else []),
                   cwd=ROOT, check=True)
    cases = [(setting, {}, code) for setting, code in
             [('zh-CN', 'zh-Hans'), ('zh-TW', 'zh-Hant'), ('zh-Hans', 'zh-Hans'),
              ('zh-Hant', 'zh-Hant'), ('en-US', 'en-US'), ('ja', 'ja'), ('unknown', 'en-US'),
              ('pt-BR', 'en-US')]]
    if sys.platform.startswith('linux'):
        for locale in ['zh_CN.UTF-8', 'zh_SG.UTF-8', 'zh-Hans', 'zh-Hans-HK', 'zh_Hans_CN', 'zh']:
            cases.append(('auto', {'LC_ALL': locale}, 'zh-Hans'))
        for locale in ['zh_TW.UTF-8', 'zh_HK.UTF-8', 'zh_MO.UTF-8', 'zh-HK', 'zh-MO',
                       'zh-Hant', 'zh-Hant-TW', 'zh_Hant_CN', 'zh_hant', 'zh_hk.UTF-8']:
            cases.append(('auto', {'LC_ALL': locale}, 'zh-Hant'))
        cases += [('auto', {'LC_MESSAGES': 'zh_HK.UTF-8', 'LANG': 'zh_CN.UTF-8'}, 'zh-Hant'),
                  ('auto', {'LANG': 'zh_MO.UTF-8'}, 'zh-Hant'),
                  ('auto', {'LC_ALL': 'zh_CN.UTF-8', 'LC_MESSAGES': 'zh_HK.UTF-8'}, 'zh-Hans'),
                  ('auto', {'LC_ALL': 'ja_JP.UTF-8'}, 'ja'), ('auto', {'LC_ALL': 'C'}, 'en-US'),
                  ('auto', {'LC_ALL': 'pt_BR.UTF-8'}, 'en-US')]
    count = 0
    for newline in ['\n', '\r\n']:
        for setting, locale, code in cases:
            ini = directory / 'test.ini'
            lines = [f'language = {setting}', 'aspect = unlocked', 'aspect_asked = 0', 'upscaler = bilinear',
                     'prompts = xbox', 'volume = 1.25', 'volume =', 'language =']
            ini.write_bytes((newline.join(lines) + newline).encode())
            env = dict(os.environ, LARECOMP_SETTINGS=str(ini), LC_ALL='', LC_MESSAGES='', LANG='C')
            env.update(locale)
            subprocess.run([str(exe), code], env=env, check=True, capture_output=True)
            saved = ini.read_text()
            expected = 'auto' if setting in ['auto', 'unknown'] else setting if setting == 'pt-BR' else code  # (the menus' only)
            assert f'language = {expected}\n' in saved
            assert 'aspect' not in saved  # (an old file's aspect lines are read past, not kept)
            count += 1
    # the upscaler's default follows the GPU until the player picks one (settings_resolve_defaults)
    for lines, mode in [(['upscaler = auto'], 'upscaler-auto'), (['volume = 1'], 'upscaler-auto'),
                        (['upscaler = fsr1'], 'upscaler-chosen'), (['upscaler = metalfx'], 'upscaler-chosen')]:
        ini = directory / 'test.ini'
        ini.write_text('\n'.join(lines) + '\n')
        subprocess.run([str(exe), mode], env=dict(os.environ, LARECOMP_SETTINGS=str(ini)), check=True, capture_output=True)
        if mode == 'upscaler-auto':
            assert 'upscaler = auto\n' in ini.read_text()
        count += 1
    # likewise the resolution: Match display until the player picks one (also on integrated GPUs); a saved one is kept
    for lines, mode, saved in [(['resolution = auto'], 'res-auto', 'auto'), (['volume = 1'], 'res-auto', 'auto'),
                               (['resolution = match'], 'res-chosen', 'match'), (['resolution = 1x'], 'res-chosen', '1x'),
                               (['resolution = 75%'], 'res-chosen', '75%'), (['resolution = dynamic'], 'res-chosen', 'match')]:
        ini = directory / 'test.ini'
        ini.write_text('\n'.join(lines) + '\n')
        env = dict(os.environ, LARECOMP_SETTINGS=str(ini))
        env.pop('LARECOMP_INTEGRATED', None)
        subprocess.run([str(exe), mode], env=env, check=True, capture_output=True)
        assert f'resolution = {saved}\n' in ini.read_text(), (lines, ini.read_text())
        # (Dynamic was a resolution in older files: it's its own toggle now, over Match display)
        assert f'dynamic_resolution = {int(lines == ["resolution = dynamic"])}\n' in ini.read_text(), (lines, ini.read_text())
        count += 1
    # Shadows (issue #602): Default unless the file says 0.5x, 2x, 3x, 4x or off ("high" from earlier test builds is 2x)
    for lines, saved in [([], 'default'), (['shadows = default'], 'default'), (['shadows = 2x'], '2x'), (['shadows = 3x'], '3x'),
                         (['shadows = 4x'], '4x'), (['shadows = off'], 'off'), (['shadows = 0.5x'], '0.5x'), (['shadows = high'], '2x'), (['shadows = ultra'], 'default')]:
        ini = directory / 'test.ini'
        ini.write_text('\n'.join(['resolution = 1x'] + lines) + '\n')
        subprocess.run([str(exe), 'res-chosen'], env=dict(os.environ, LARECOMP_SETTINGS=str(ini)), check=True, capture_output=True)
        assert f'shadows = {saved}\n' in ini.read_text(), (lines, ini.read_text())
        count += 1
    print(f'{count} parser, locale, upscaler / resolution default and roundtrip cases passed (LF/CRLF)')
