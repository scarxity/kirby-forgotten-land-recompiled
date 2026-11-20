#!/usr/bin/env python3
"""Fails when the places that name the game disagree: game/game.h and game.mk are the source, and runtime/game_addresses/
versions.h gives the build ids. The launcher (its Rust, its Tauri config and package files), recomplauncher.json, the
README, the release README and the CHANGELOG each repeat some of those values, and a new game's port starts by changing
them all.

  python tools/check_game_identity.py            the repository
  python tools/check_game_identity.py --dist D   also the staged release folder D (make dist)
"""
import json
import os
import re
import sys

SETTING_READS = re.compile(r'strcmp\(key, "([a-z_0-9]+)"\)')


def read(root, path):
    with open(os.path.join(root, path), encoding='utf-8') as f:
        return f.read()


def find(pattern, text, what):
    m = re.search(pattern, text, re.M)
    if not m:
        raise ValueError(f'{what} not found')
    return m.group(1)


def source(root):
    """What game/ and versions.h say."""
    h, mk = read(root, 'game/game.h'), read(root, 'game/game.mk')
    modules = re.findall(r'X\((\w+),\s*0x[0-9a-fA-F]+\)', find(r'^#define GAME_MODULES\(X\)(.*)$', h, 'GAME_MODULES'))
    return {
        'name': find(r'^#define GAME_NAME "([^"]+)"', h, 'GAME_NAME'),
        'repo': find(r'^#define GAME_REPO "([^"]+)"', h, 'GAME_REPO'),
        'program_id': int(find(r'^#define GAME_PROGRAM_ID (0x[0-9a-fA-F]+)', h, 'GAME_PROGRAM_ID'), 16),
        'markers': re.findall(r'"(\w+)"', find(r'^#define GAME_ROMFS_MARKERS (.*)$', h, 'GAME_ROMFS_MARKERS')),
        'modules': modules,
        'exe': find(r'^EXE_NAME\s*:?=\s*(\S+)', mk, 'EXE_NAME'),
        'version': find(r'^GAME_VERSION\s*:?=\s*(\S+)', mk, 'GAME_VERSION'),
        'build_ids': [b.lower() for b in re.findall(r'GA_VERSION\(\w+,\s*"[^"]*",\s*"([0-9A-Fa-f]{40})"\)',
                                                    read(root, 'runtime/game_addresses/versions.h'))],
    }


def rust_const(rs, name):
    return find(rf'^const {name}: [^=]+= (.+);$', rs, name)


def check(root):
    src = source(root)
    errors = []

    def eq(what, got, want):
        if got != want:
            errors.append(f'{what}: {got!r}, game/ says {want!r}')

    pid = f'{src["program_id"]:016X}'
    exe = src['exe'] + '.exe'

    # the launcher
    rs = read(root, 'launcher/src-tauri/src/main.rs')
    eq('launcher main.rs PROGRAM_ID', int(rust_const(rs, 'PROGRAM_ID').replace('_', ''), 16), src['program_id'])
    eq('launcher main.rs UPDATE_ID', int(rust_const(rs, 'UPDATE_ID').replace('_', ''), 16), src['program_id'] + 0x800)
    eq('launcher main.rs PACKAGED_EXE', rust_const(rs, 'PACKAGED_EXE').strip('"'), exe)
    eq('launcher main.rs MODULES', re.findall(r'"(\w+)"', rust_const(rs, 'MODULES')), src['modules'])
    eq('launcher main.rs RomFS markers', re.findall(r'"(\w+)"', find(r'for d in \[([^\]]+)\] \{\s*if !dir\.join\("romfs"\)', rs, 'markers')),
       src['markers'])
    tauri = json.loads(read(root, 'launcher/src-tauri/tauri.conf.json'))
    eq('tauri.conf.json version', tauri['version'], src['version'])
    eq('tauri.conf.json window title', tauri['app']['windows'][0]['title'], src['name'])
    eq('launcher/package.json version', json.loads(read(root, 'launcher/package.json'))['version'], src['version'])
    lock = json.loads(read(root, 'launcher/package-lock.json'))
    eq('launcher/package-lock.json version', lock['version'], src['version'])
    eq('launcher/package-lock.json root package version', lock['packages']['']['version'], src['version'])
    eq('Cargo.toml version', find(r'^version = "([^"]+)"', read(root, 'launcher/src-tauri/Cargo.toml'), 'Cargo.toml version'),
       src['version'])
    eq('Cargo.lock version', find(r'name = "kfl-recomp-launcher"\nversion = "([^"]+)"', read(root, 'launcher/src-tauri/Cargo.lock'),
                                  'Cargo.lock'), src['version'])

    # RecompLauncher
    man = json.loads(read(root, 'recomplauncher.json'))
    eq('recomplauncher.json name', man['name'], src['name'])
    eq('recomplauncher.json repo', man['repo'], src['repo'])
    eq('recomplauncher.json prepare program', man['prepare']['program'], [exe])
    eq('recomplauncher.json start program', man['start']['program'], [exe])
    if man['game']['check']['nup_build_id'].lower() not in src['build_ids']:
        errors.append(f'recomplauncher.json nup_build_id {man["game"]["check"]["nup_build_id"]} is not in versions.h {src["build_ids"]}')
    missing = [c for c in map(str, range(8, 16)) if c not in man['exit_codes']]
    if missing:
        errors.append(f'recomplauncher.json exit_codes lacks {missing} (docs/launcher-runtime-contract.md)')

    # the settings both launchers write must be ones the runtime reads
    keys = set(SETTING_READS.findall(read(root, 'runtime/settings.c')))
    # (rows marked pref: true are the launcher's own, in launcher-prefs.json)
    written = {m[0] for m in re.findall(r"\bkey: '(\w+)'([^\n]*)", read(root, 'launcher/src/settings.js')) if 'pref: true' not in m[1]}
    for s in man['settings']:
        written |= {k for c in s.get('choices', []) for k in c['set']} | set(s.get('defaults', {}))
        if 'slider' in s:
            written.add(s['slider']['key'])
    if written - keys:
        errors.append(f'the launchers write settings the runtime does not read: {sorted(written - keys)}')

    # player text
    for path in ('README.md', 'docs/release-README.txt'):
        text = read(root, path)
        if pid not in text.upper():
            errors.append(f'{path} does not name the program id {pid}')
    readme = read(root, 'README.md')
    for b in src['build_ids']:
        if b[:16] not in readme:
            errors.append(f'README.md does not name the main build id {b[:16]}')
    release = read(root, 'docs/release-README.txt')
    for name in (exe, 'ForgottenLandLauncher.exe'):
        if name not in release:
            errors.append(f'docs/release-README.txt does not name {name}')
    top = find(r'^## (\d+\.\d+\.\d+)', read(root, 'CHANGELOG.md'), 'CHANGELOG version')
    eq('CHANGELOG.md newest version', top, src['version'])
    return errors


def check_dist(root, dist):
    src = source(root)
    errors = []
    for name in (src['exe'] + '.exe', 'ForgottenLandLauncher.exe', 'lasrecomp.dll.a', 'README.txt', 'LICENSE.txt'):
        if not os.path.isfile(os.path.join(dist, name)):
            errors.append(f'{dist} has no {name}')
    if os.path.isfile(os.path.join(dist, 'recomplauncher.json')):
        if read(dist, 'recomplauncher.json') != read(root, 'recomplauncher.json'):
            errors.append('the staged recomplauncher.json is not the repository one')
    else:
        errors.append(f'{dist} has no recomplauncher.json')
    if os.path.isfile(os.path.join(dist, 'README.txt')) and f'{src["program_id"]:016X}' not in read(dist, 'README.txt').upper():
        errors.append('the staged README.txt does not name the program id')
    return errors


def main(argv):
    root = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
    errors = check(root)
    if len(argv) == 3 and argv[1] == '--dist':
        errors += check_dist(root, argv[2])
    for e in errors:
        print('check_game_identity:', e)
    if not errors:
        print('check_game_identity: ok')
    return 1 if errors else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
