Forgotten Land Recomp
=====================

A native Windows build of Kirby and the Forgotten Land (Nintendo Switch, version 1.0.0), made by static
recompilation: the game's code is translated into C and compiled into a program for your PC. It is not an emulator.
Project page and source code: https://github.com/scarxity/kirby-forgotten-land-recompiled

This is a work in progress. docs/STATUS.md in the repository says what has been checked and the known problems.

NO GAME FILES ARE INCLUDED, and no game code: this zip holds only tools. You need your own copy of the game, version
1.0.0, extracted to a folder that holds two folders, exefs and romfs (program id 01004D300C5AE000). Packed files
(.xci, .nsp, .nsz, .nca) must be extracted first. The game's update is not used. The game code is built on your PC
from those files, and stays there.

What you need
-------------
- Windows 10 or 11, 64-bit.
- A CPU with AVX2 (Intel Haswell, 2013, or newer; AMD Excavator or Zen, or newer). The game code is built for it.
- A graphics card whose driver has Vulkan 1.3.
- The Microsoft Edge WebView2 runtime, which the launcher window uses. Windows 11 has it. Most Windows 10 PCs have it
  too. If the launcher does not open, install it from https://developer.microsoft.com/microsoft-edge/webview2/
- 6.5 GB of free disk, on top of this folder's 272 MB. The launcher checks it before it builds (see Disk space).
- A short folder path, under 200 characters (for example C:\Games\ForgottenLandRecomp). Windows stops the build at
  long paths, so the launcher refuses one before it starts.

How to start
------------
1. Unzip this folder anywhere you can write to (not inside Program Files). Keep the files together.
2. Open ForgottenLandLauncher.exe. It is not code-signed. If Windows says "Windows protected your PC", click
   "More info" and then "Run anyway". Do the same if it asks about ForgottenLandRecomp.exe.
3. Click "Select game folder" and pick the folder that holds your exefs and romfs folders.
4. Click "Set up" (on the Set up page, "Build"). The launcher puts the game files into this folder, then translates
   the game code and compiles it for your PC. This takes a while, once: about 7 minutes on a 16-thread PC. Fewer
   cores take longer. A file that takes over a minute is named under the progress bar. You can Cancel and build again
   later: the pieces already compiled are kept, so it carries on from there.
5. Click "Play". Next time the launcher shows Play straight away.

The first time you reach an area, the picture may stop for a moment while the game builds that area's shaders. Wait
for it: the game goes on by itself. On a slow PC this can take 15 to 25 seconds. The shaders are kept in the cache
folder, so later visits pause about a second. Inside an area, something you see for
the first time (a new effect or enemy) may appear a frame late instead of stopping the game.

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
Keys and buttons can be changed in the menu, on its Controls page. The changes are saved in larecomp.ini as
bind_key_... and bind_pad_... lines. A controller that SDL does not know can be added with a gamecontrollerdb.txt in
this folder.

To quit, open the menu and choose Quit, or close the window.

Mods
----
You can install mods of your own choosing. This project does not make, include or distribute any, and a mod is not
part of this project: you get it from its author and use it at your own risk. An example is a 60fps mod by Lhw-Arthas
(PR #9 in the repository, not included here).
- A mod is a folder that holds a romfs folder. Put it in the mods folder here (the launcher's Settings page has
  "Open mods folder"), so that a file such as mods\<mod name>\romfs\basil\Scn.bin replaces the game's file of the same
  path in romfs.
- The launcher lists the mods on the Settings page. Switch one off with its box, or move it up or down. When two mods
  have the same file, the one higher in the list wins. The launcher writes the list to mods\mods.txt: one mod name a
  line, in order, and "-<mod name>" for a mod switched off. You can edit it by hand. Mods it does not list come after
  the listed ones, in name order, switched on.
- Changes apply the next time the game starts. The launcher keeps the list locked while the game or a build is running.
- To turn a mod off, clear its box. To remove it, delete its folder. The game files in extracted\ are never changed.
- If the game misbehaves, turn all mods off before you report a problem.

Updating
--------
Unzip a new version over the old folder. If the new version needs the game code built again, the launcher says so and
offers "Rebuild game code" (a few minutes; your game folder is not needed again).
Coming from 0.2.0, the rebuild also links the game folder you picked then, in place of the romfs.bin 0.2.0 made (it
deletes that file and frees about 6 GB). If that folder was moved or deleted, pick it again first.

Closing the launcher ends a build in progress and every tool the build started (build again later: it carries on
from the pieces already compiled). A game that is running keeps running.

Disk space
----------
The game's data (the romfs, about 6 GB) and the game code (about 120 MB) are kept in the extracted folder here. When
your romfs folder (or a romfs.bin you picked instead) is on the same drive as this folder, it is hard-linked, not
copied, and takes no extra space. On another drive it is copied and takes about 6 GB. The launcher cannot tell in
advance which one will happen, so it asks for 6.5 GB free either way (1 GB to rebuild only the game code). The
translated code and the compiler's files take a few hundred MB more and stay in extracted\.gamebuild, so a later
build or an update compiles only what changed. After a successful build this copy no longer needs the folder you
picked.

Where things are
----------------
Everything stays in this folder:
- extracted\        the game files, the game code built from them and the build's files (yours: never share them)
- larecomp.ini      settings (the launcher's Settings page and the in-game menu change them)
- save\             saves
- mods\             the mods you installed, and mods.txt, their order
- cache\            shader caches
- game.log          the log of the last time the game ran; setup.log, the log of the last build
- crash-*.log       the logs of the last five crashes (the launcher shows a notice when the game crashes)
- game-folder.txt   the game folder you picked
To start over, delete the extracted folder (and save\ if you also want to drop your saves).

If something goes wrong
-----------------------
- The game stopped: the launcher's Home page says why and keeps a copy of the log as crash-<date>-<time>.log. Use
  "Open crash log" to read it. To report a problem, attach that file or game.log: it names your GPU and driver.
- The build failed: the launcher says why, and "Open setup log" shows setup.log, the build's own output.
- The launcher says "Rebuild game code": the game code was built by another version. Click it.
- The launcher says files are missing, or the build stops at once: an antivirus may have removed or blocked
  lasgen\lasgen.exe or zig\zig.exe. Restore them (or unzip again) and allow this folder in the antivirus.
- "That is the game's update, not the base game", or "not the files of Kirby and the Forgotten Land 1.0.0":
  extract the base game, version 1.0.0, without the update, and pick that folder.
- A message about Vulkan or the graphics driver: update your graphics driver. The game needs Vulkan 1.3.
- A message about AVX2: this CPU is too old for the game code.
- The game closes within seconds: the launcher says so. Read game.log for the reason.

Legal and licences
------------------
Fan project, not affiliated with Nintendo or HAL Laboratory. MIT licensed (LICENSE.txt). The runtime and the game code
generator are built on LAS Recomp by Peiikai (https://github.com/peiikai/LASRecomp, MIT,
licenses\LASRecomp-MIT.txt), and the port started from letsgo-recomp by raulzrrs
(https://github.com/raulzrrs/letsgo-recomp, MIT). The reverb and delay sound effects and parts of the shader
translator are adapted from Ryujinx (MIT, licenses\Ryujinx-MIT.txt). The launcher's layout follows Recompia's
(https://github.com/Dittobyte/Recompia); none of its code or art is used. Thanks to Lhw-Arthas for the Chinese
language fix.

Bundled third-party software, each licence in the licenses folder:
- FFmpeg __FFVER__ (LGPL 2.1): avcodec, avformat, avutil, swresample, swscale DLLs, built with only the decoders the
  game's movies need. Source: https://ffmpeg.org/releases/ffmpeg-__FFVER__.tar.xz, built with MSYS2
  UCRT64 clang (tools/build_deps_windows.sh in the repository) as:
__FFCONF__
  To use another build, replace the DLLs with ones built from the same version.
- SDL3 (zlib), shaderc with glslang and SPIRV-Tools (Apache 2.0 and others), libiconv (LGPL), the GCC runtime
  libgcc and libstdc++ (GPL 3 with the GCC Runtime Library Exception) and winpthreads (MIT): from MSYS2 UCRT64.
- Dear ImGui (MIT), SMAA (MIT) and code adapted from Ryujinx (MIT), compiled into ForgottenLandRecomp.exe.
- The Baloo 2 font (SIL Open Font License 1.1) and the DejaVu Sans fonts (free licence, DejaVu-LICENSE.txt).
- zig\: Zig (MIT), which contains LLVM, clang and lld (Apache 2.0 with LLVM exception), with clang's headers and the
  MinGW-w64 headers and runtime (MinGW-w64 licences).
- lasgen\: the project's game code generator frozen with PyInstaller (bootloader: GPL 2 with an exception that
  allows this use), with Python (PSF licence, which also covers the libraries Python bundles), capstone
  (BSD 3-Clause), unicorn (GPL 2) and python-lz4 (BSD 3-Clause). Source of unicorn:
  https://github.com/unicorn-engine/unicorn (version 2.1.4).
- ForgottenLandLauncher.exe: built with Tauri; every Rust crate and JavaScript package in it, with its licence, is in
  licenses\ForgottenLandLauncher-third-party.txt.
