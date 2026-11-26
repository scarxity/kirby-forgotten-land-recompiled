Forgotten Land Recomp __VERSION__ for Linux
==========================================

A native Linux build of Kirby and the Forgotten Land (Nintendo Switch, version 1.0.0), made by static recompilation: the
game's code is translated into C and compiled into a program for your PC. It is not an emulator.
Project page and source code: https://github.com/scarxity/kirby-forgotten-land-recompiled

THIS LINUX BUILD IS UNTESTED. It was built and checked by a script in a container (it compiles, the game-free tests
pass), but nobody has played it on Linux yet. Feedback is welcome: open an issue on the project page and attach
game.log (see "Where things are"). The Windows build is the tested one.

NO GAME FILES ARE INCLUDED, and no game code or assets: this AppImage holds only tools, and a list of shader variant
records (one-way hashes and draw settings, never shader code) that the shader step uses to find and build the shaders
in your own game files. You need your own copy of the game, version 1.0.0, extracted to a folder that holds two folders,
exefs and romfs (program id 01004D300C5AE000). Packed files (.xci, .nsp, .nsz, .nca) must be extracted first. The game's
update is not used. The game code is built on your PC from those files, and stays there.

What you need
-------------
- 64-bit Linux with glibc 2.35 or newer (Ubuntu 22.04 or newer, or the same age), on an x86-64 CPU with AVX2 (Intel
  Haswell, 2013, or newer; AMD Excavator or Zen, or newer). The game code is built for it.
- A graphics card whose driver has Vulkan 1.3 (Mesa RADV or ANV, or NVIDIA's driver).
- 6.5 GB of free disk. A game folder on the same file system is linked, not copied.
- zenity (optional): the AppImage shows its hints in a window when it is started from a file manager.

How to start
------------
1. Make the file executable: chmod +x ForgottenLandRecomp-__VERSION__-Linux-x86_64.AppImage
2. Set the game up, once. In a terminal, with the folder that holds your exefs and romfs:
     ./ForgottenLandRecomp-__VERSION__-Linux-x86_64.AppImage --prepare /path/to/your/game/folder
   It puts the game files into the data folder (below), translates the game code and compiles it for your PC, then
   builds the shaders for your graphics card. About 7 minutes for the code on a 16-thread PC, and about 15 more for the
   shaders. It prints "progress <0 to 1> <what it is doing>" and ends with "progress 1 Ready". You can stop it and run
   it again: the pieces already compiled are kept. A failure ends with an exit code: 8 no game files in that folder,
   9 a .nsp or .xci (extract it first), 10 not version 1.0.0, 11 copying failed (disk space), 12 the game code did not
   build (see game.log in the data folder), 13 tools missing.
3. Play: start the AppImage again with no arguments, or from your file manager.
RecompLauncher (https://recomplauncher.com) can set the game up and start it too, with this AppImage's
recomplauncher.json.

After an update of this program, or a graphics driver update, run --prepare again (it builds the shaders again).

At the online prompt, choose "Do Not Use": online features do not work.

Controls
--------
A controller works as the Switch's: the buttons by position (the right button is A, the bottom one B). For the
letters printed on an Xbox controller instead, set "face_buttons_by_label = 1" in larecomp.ini. The keyboard:
  W A S D             move (left stick)        arrow keys         d-pad
  K / J / L / I       A / B / X / Y            Q / E              L / R
  1 / 3               ZL / ZR                  Enter / Backspace  Plus (pause) / Minus
  Numpad 8 4 5 6      right stick              F / Numpad 0       left / right stick press
  Esc or F1           the menu: settings, Controls, Reset, Quit
  F11 or Alt+Enter    full screen
On a controller, the Home button, or Back and Start together, opens the menu.
Keys and buttons can be changed in the menu, on its Controls page. To quit, open the menu and choose Quit, or close
the window.

Where things are
----------------
Everything the game writes goes to ~/.local/share/ForgottenLandRecomp (or $XDG_DATA_HOME/ForgottenLandRecomp), or the
folder LARECOMP_DATA_ROOT names:
- extracted/        the game files, the game code built from them and the build's files (yours: never share them)
- larecomp.ini      settings (the in-game menu changes them)
- save/             saves
- mods/             mods you installed: mods/<name>/romfs/... replaces the game's file of the same path
- cache/            shader caches
- game.log          the log of the last run, of the game or of --prepare (game-backup-*.log: the earlier ones)
To start over, delete the extracted folder (and save/ if you also want to drop your saves).

If something goes wrong
-----------------------
- Attach game.log (and cache/shader_seen.txt for stutter) to an issue: it names your GPU and driver.
- A message about Vulkan or the graphics driver: update your graphics driver and Mesa. The game needs Vulkan 1.3.
- On AMD graphics cards the game may need the fix of pull request #21 in the repository, which this version may not have.
- A message about AVX2: this CPU is too old for the game code.
- The build failed: game.log in the data folder (the log of the run that failed) says why.

Legal and licences
------------------
Fan project, not affiliated with Nintendo or HAL Laboratory. MIT licensed (LICENSE.txt). The runtime and the game code
generator are built on LAS Recomp by Peiikai (https://github.com/peiikai/LASRecomp, MIT, licenses/LASRecomp-MIT.txt),
and the port started from letsgo-recomp by raulzrrs (https://github.com/raulzrrs/letsgo-recomp, MIT). The reverb and
delay sound effects and parts of the shader translator are adapted from Ryujinx (MIT, licenses/Ryujinx-MIT.txt). The
Linux build follows the fixes saschb2b reported in issue #8 (zstd for the shader preparation, FFmpeg's Opus decoder).

Bundled third-party software, each licence in the licenses folder:
- FFmpeg __FFVER__ (LGPL 2.1): libavcodec, libavformat, libavutil, libswresample, libswscale, built with only the
  decoders the game's movies and sound need. Source: https://ffmpeg.org/releases/ffmpeg-__FFVER__.tar.xz, built with
  clang (tools/linux/build_deps.sh in the repository) as:
__FFCONF__
  To use another build, replace the libraries in usr/lib of the unpacked AppImage (--appimage-extract) with ones built
  from the same version.
- SDL3 (zlib), shaderc with glslang and SPIRV-Tools (Apache 2.0 and others), built from source for this AppImage.
  zstd (BSD) is linked into the runtime.
- Dear ImGui (MIT), SMAA (MIT) and code adapted from Ryujinx (MIT), compiled into ForgottenLandRecomp.
- The Baloo 2 font (SIL Open Font License 1.1) and the DejaVu Sans fonts (free licence, DejaVu-LICENSE.txt).
- zig/: Zig (MIT), which contains LLVM, clang and lld (Apache 2.0 with LLVM exception).
- lasgen/: the project's game code generator frozen with PyInstaller (bootloader: GPL 2 with an exception that allows
  this use), with Python (PSF licence), capstone (BSD 3-Clause), unicorn (GPL 2) and python-lz4 (BSD 3-Clause). Source
  of unicorn: https://github.com/unicorn-engine/unicorn (version 2.1.4).
