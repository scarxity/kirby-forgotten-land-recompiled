// Launcher backend. It never uploads or bundles game files: the game code is generated and compiled on this PC from the
// folder the player picks. Beside a source checkout that folder is linked into the work folder (a directory junction);
// in the release zip the runtime's own setup copies or hard-links it into the install folder.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod update;

use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Kirby and the Forgotten Land, any region build of the base game (main.npdm's program id).
const PROGRAM_ID: u64 = 0x0100_4D30_0C5A_E000;
/// The game's update (patch), which has the same title and a program id ending E800.
const UPDATE_ID: u64 = 0x0100_4D30_0C5A_E800;
const MODULES: [&str; 3] = ["main", "sdk", "subsdk0"];
const SETTING_KEYS: [&str; 3] = ["fullscreen", "resolution", "volume"];
/// The runtime (game/game.mk EXE_NAME): next to the launcher in the zip, in build/ beside a source checkout.
const PACKAGED_EXE: &str = "ForgottenLandRecomp.exe";
/// Packaged mode: the folder the player picked, remembered until Build reads it.
const PICKED_FILE: &str = "game-folder.txt";
/// Free disk space the build needs (about 6.4 GB were in use at the peak of a real one, the game files copied: a hard
/// link on the same drive costs none of that, but the launcher cannot know beforehand), and the game code alone.
const BUILD_BYTES: u64 = 6_500_000_000;
const CODE_BYTES: u64 = 1_000_000_000;
/// The longest install folder path the build works from. Measured with the bundled tools: the build failed to start its
/// tools from 216 characters on, because Windows paths stop at 260 and the build adds about 45 below this folder.
const MAX_WORK_PATH: usize = 200;
/// A game that ends sooner than this after it started did not get anywhere: the launcher says so.
const EARLY_EXIT: Duration = Duration::from_secs(15);
/// What the shaders were last prepared for (shader_stamp), in the work folder
const SHADER_STAMP: &str = "shaders-prepared.txt";
/// The launcher in the release zip (tools/dist.mk), what an update starts again.
const LAUNCHER_EXE: &str = "ForgottenLandLauncher.exe";

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    #[default]
    Idle,
    Runtime,
    /// Packaged mode: the runtime looking for the game files and copying them. Developer mode: checking the dump.
    Files,
    Translate,
    Compile,
    /// The game started hidden to prepare every shader of the game files, so play compiles none (runtime/nvn/shader_prep.c).
    Shaders,
    Failed,
}

#[derive(Default)]
struct Job {
    phase: Phase,
    /// Packaged mode: the runtime's own fraction of the whole setup.
    progress: Option<f32>,
    /// Packaged mode: the file the compiler has been on for a while, as the runtime words it.
    detail: Option<String>,
    error: String,
    child: Option<Child>,
    cancelled: bool,
}

/// Beside a source checkout: the runtime and the game code are built with make through MSYS2.
struct Dev {
    repo: PathBuf,
    msys: PathBuf,
}

/// Decided once at startup from what sits next to the launcher.
enum Mode {
    Dev(Dev),
    /// The release zip: the runtime and the tools it builds the game code with are next to the launcher, and the
    /// install folder is the work folder (game files, saves, settings, logs).
    Packaged,
}

impl Mode {
    fn detect(exe_dir: &Path) -> Mode {
        let tools = exe_dir.join("lasgen").join("lasgen.exe").is_file() && exe_dir.join("zig").join("zig.exe").is_file();
        // a release folder with its tools removed (or the launcher started from inside the zip) is still a release folder
        match find_repo(exe_dir) {
            Some(repo) if !tools => Mode::Dev(Dev {
                repo,
                msys: std::env::var("KFL_MSYS2").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(r"C:\msys64")),
            }),
            _ => Mode::Packaged,
        }
    }
}

/// One stdout line of the runtime's `--setup` (runtime/setup_cli.c) or `--check`.
#[derive(Debug, PartialEq)]
enum SetupLine {
    Progress(Phase, f32, Option<String>),
    Done,
    Failed(String),
}

fn parse_setup_line(line: &str) -> Option<SetupLine> {
    let mut w = line.split_whitespace();
    match w.next()? {
        "phase" => {
            let phase = match w.next()? {
                "looking" | "copying" => Phase::Files,
                "reading" => Phase::Translate,
                "compiling" | "finishing" => Phase::Compile,
                "shaders" => Phase::Shaders,
                _ => return None,
            };
            let fraction = w.next()?.parse().ok()?;
            let detail = w.collect::<Vec<_>>().join(" ");
            Some(SetupLine::Progress(phase, fraction, (!detail.is_empty()).then_some(detail)))
        }
        "done" | "ok" => Some(SetupLine::Done),
        "failed" => Some(SetupLine::Failed(w.next().unwrap_or("").into())),
        _ => None,
    }
}

fn setup_failure(reason: &str) -> String {
    match reason {
        "not_found" => "No game files were found in that folder. It needs your extracted exefs (it holds main and main.npdm) and romfs (it holds the folders msg and gfx), in it or in folders inside it.",
        "package" => "Those are packed game files (.xci, .nsp). Extract them first (exefs and romfs).",
        "version" => "Those are not the files of Kirby and the Forgotten Land 1.0.0. They are from another game, or from a version of Kirby this release does not support. Extract version 1.0.0 of the base game.",
        "update" => "That is the game's update, not the base game. Extract the base game, version 1.0.0, without the update.",
        "copy" => "The game files could not be copied. Check the free disk space and try again.",
        "tools" => "The build tools are missing from this folder. An antivirus may have removed them. Unzip the release again and allow this folder in your antivirus.",
        "bad_image" => "The game's romfs could not be read. If you picked a romfs.bin, it is cut short or is not a plain RomFS image. Extract the romfs folder again and pick the folder that holds it.",
        _ => "The game code did not build. Open the setup log for the details. If an antivirus is running, it may have blocked the compiler: allow this folder in it and build again.",
    }
    .into()
}

/// How the game ended, from its exit code and the end of game.log.
#[derive(Debug, PartialEq)]
enum Outcome {
    Quit,
    /// Non-zero exit with no crash report: the player or Windows ended it, or it failed at startup with its own message box.
    Stopped,
    /// The one-line reason.
    Crashed(String),
}

/// The last crash, until the player dismisses it or presses Play. `log` is a file name inside the work folder.
#[derive(Clone, Serialize)]
struct Crash {
    reason: String,
    log: String,
}

/// What the runtime says is set up in the work folder (`--status`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Code {
    None,
    /// The game files are there, but the game code is missing or was built by another version of the runtime.
    Build,
    Ready,
}

fn parse_status(stdout: &str) -> Code {
    match stdout.lines().map(str::trim).find(|l| matches!(*l, "ready" | "build" | "none")) {
        Some("ready") => Code::Ready,
        Some("build") => Code::Build,
        _ => Code::None,
    }
}

/// The Windows calls the launcher needs.
#[cfg(windows)]
mod win {
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::process::Child;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows_sys::Win32::System::JobObjects::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// A job that ends every process in it when the last handle to it closes.
    pub fn kill_on_close_job() -> HANDLE {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const _, std::mem::size_of_val(&info) as u32);
            job
        }
    }

    pub fn join(job: HANDLE, child: &Child) -> bool {
        unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) != 0 }
    }

    /// The child ends with the launcher however the launcher ends (closed, crashed, killed): the job's only handle is
    /// the launcher's own, which Windows closes with the process. Everything the child starts is in the job too.
    pub fn end_with_launcher(child: &Child) {
        static JOB: OnceLock<usize> = OnceLock::new();
        join(*JOB.get_or_init(|| kill_on_close_job() as usize) as HANDLE, child);
    }

    pub fn free_bytes(dir: &Path) -> Option<u64> {
        let mut free = 0u64;
        let ok = unsafe { GetDiskFreeSpaceExW(wide(&dir.to_string_lossy()).as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
        (ok != 0).then_some(free)
    }
}

struct App {
    mode: Mode,
    work: PathBuf,
    /// Why this folder cannot be built in (checked once at startup).
    work_problem: Option<String>,
    job: Arc<Mutex<Job>>,
    /// The running game and when it started.
    game: Mutex<Option<(Child, Instant)>>,
    crash: Mutex<Option<Crash>>,
    /// Why the last folder the player picked was not used, until the next pick.
    pick_error: Mutex<String>,
    /// The runtime's verdict, with the times of the files it was made from.
    code: Mutex<Option<([Option<SystemTime>; 5], Code)>>,
    updater: update::Updater,
}

#[derive(Serialize)]
struct Status {
    work: String,
    game_dir: Option<String>,
    game_error: Option<String>,
    tools_missing: Vec<String>,
    work_error: Option<String>,
    built: bool,
    /// The game files are set up and only the game code needs building (for this version).
    rebuild: bool,
    running: bool,
    phase: Phase,
    progress: Option<f32>,
    detail: Option<String>,
    error: String,
    settings: Vec<(String, String)>,
    mods: Vec<ModEntry>,
    crash: Option<Crash>,
    packaged: bool,
    /// Play prepares the shaders first (shader_stamp)
    shaders_ready: bool,
    update: update::Update,
    version: &'static str,
    auto_update: bool,
    stable_only: bool,
}

fn find_repo(exe_dir: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KFL_REPO") {
        return Some(PathBuf::from(p));
    }
    exe_dir.ancestors().find(|d| d.join("Makefile").is_file() && d.join("runtime").is_dir()).map(Path::to_path_buf)
}

fn program_id(npdm: &Path) -> Option<u64> {
    let b = fs::read(npdm).ok()?;
    if b.get(..4)? != b"META" {
        return None;
    }
    let aci = u32::from_le_bytes(b.get(0x70..0x74)?.try_into().ok()?) as usize;
    if b.get(aci..aci + 4)? != b"ACI0" {
        return None;
    }
    Some(u64::from_le_bytes(b.get(aci + 0x10..aci + 0x18)?.try_into().ok()?))
}

fn check_game(dir: &Path) -> Result<(), String> {
    let layout = "Pick the folder that holds your extracted exefs (with main, sdk, subsdk0 and main.npdm) and romfs (with the folders msg and gfx), right inside it.";
    let exefs = dir.join("exefs");
    for m in MODULES {
        if !exefs.join(m).is_file() {
            return Err(format!("This folder has no exefs\\{m}. {layout}"));
        }
    }
    for d in ["msg", "gfx"] {
        if !dir.join("romfs").join(d).is_dir() {
            return Err(format!("This folder has no romfs\\{d}. {layout}"));
        }
    }
    match program_id(&exefs.join("main.npdm")) {
        Some(PROGRAM_ID) => Ok(()),
        Some(UPDATE_ID) => Err(setup_failure("update").into()),
        Some(id) => Err(format!("{} Its program id is {id:016X}.", setup_failure("version"))),
        None => Err("exefs\\main.npdm is missing or unreadable. Extract the exefs again.".into()),
    }
}

/// Packaged mode: the runtime's own search decides (`--check`), so the launcher never disagrees with the setup.
fn runtime_check(app: &App, dir: &Path) -> Result<(), String> {
    let mut c = app.runtime();
    c.arg("--check").arg(dir).stderr(Stdio::null());
    hide_window(&mut c);
    let out = c.output().map_err(|e| format!("Could not check that folder: {e}"))?;
    check_verdict(&String::from_utf8_lossy(&out.stdout))
}

fn check_verdict(stdout: &str) -> Result<(), String> {
    match stdout.lines().find_map(parse_setup_line) {
        Some(SetupLine::Done) => Ok(()),
        Some(SetupLine::Failed(reason)) => Err(setup_failure(&reason)),
        _ => Err("The game runtime could not check that folder.".into()),
    }
}

fn count(dir: &Path, ext: &str) -> usize {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                count(&p, ext)
            } else {
                (p.extension().and_then(|x| x.to_str()) == Some(ext)) as usize
            }
        })
        .sum()
}

// The runtime prints one of two markers on a crash (runtime/rt.h FATAL, runtime/dispatch.c rt_fault_report) and then abort()s,
// which exits with code 3. A normal close (window closed, or the game asking to exit) exits with code 0.
fn classify(code: i32, tail: &str) -> Outcome {
    let marker = tail.lines().map(str::trim).find(|l| l.starts_with("FATAL:") || l.starts_with("*** exception"));
    match (marker, code) {
        (Some(line), _) => Outcome::Crashed(line.chars().take(300).collect()),
        (None, 0) => Outcome::Quit,
        (None, 3) => Outcome::Crashed("The game aborted without a report (exit code 3).".into()),
        // an NTSTATUS exit code is a native fault the runtime's handler never saw
        (None, c) if c as u32 >= 0xC000_0000 => Outcome::Crashed(format!("Windows ended the game with error {:08X}.", c as u32)),
        _ => Outcome::Stopped,
    }
}

/// What a player can do about a start-up failure the runtime words for developers (runtime/nvn/vulkan/device.c).
fn explain(reason: String) -> String {
    if reason.contains("Vulkan") || reason.contains("SDL_CreateWindow") {
        format!("Your graphics card or its driver may not support Vulkan 1.3. Update the graphics driver and try again. ({reason})")
    } else {
        reason
    }
}

const KEPT_CRASH_LOGS: usize = 5;

fn log_tail(path: &Path) -> String {
    let read = || -> std::io::Result<Vec<u8>> {
        let mut f = fs::File::open(path)?;
        f.seek(SeekFrom::End(-(1 << 16)))
            .or_else(|_| f.seek(SeekFrom::Start(0)))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    };
    String::from_utf8_lossy(&read().unwrap_or_default()).into_owned()
}

fn is_crash_log(name: &str) -> bool {
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    name.strip_prefix("crash-")
        .and_then(|r| r.strip_suffix(".log"))
        .and_then(|r| r.split_once('-'))
        .is_some_and(|(d, t)| digits(d, 8) && digits(t, 6))
}

// Copies game.log aside, because the next Play overwrites it. Returns the copy's name (game.log itself if the copy fails).
fn keep_crash_log(work: &Path) -> String {
    let name = format!("crash-{}.log", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    if fs::copy(work.join("game.log"), work.join(&name)).is_err() {
        return "game.log".into();
    }
    let mut all: Vec<String> = fs::read_dir(work).into_iter().flatten().flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| is_crash_log(n)).collect();
    all.sort();
    let old = all.len().saturating_sub(KEPT_CRASH_LOGS);
    for f in &all[..old] {
        let _ = fs::remove_file(work.join(f));
    }
    name
}

impl App {
    // true while the game runs; when it has just ended, records a crash for the status
    fn poll_game(&self) -> bool {
        let mut g = self.game.lock().unwrap();
        let Some((child, started)) = g.as_mut() else { return false };
        let ran = started.elapsed();
        let ended = match child.try_wait() {
            Ok(None) => return true,
            Ok(Some(st)) => Some(st.code().unwrap_or(1)),
            Err(_) => None,
        };
        *g = None;
        if let Some(code) = ended {
            let reason = match classify(code, &log_tail(&self.work.join("game.log"))) {
                Outcome::Crashed(reason) => Some(explain(reason)),
                _ if ran < EARLY_EXIT => Some(format!("The game closed {} seconds after it started (exit code {code}). The game log says why.", ran.as_secs())),
                _ => None,
            };
            if let Some(reason) = reason {
                *self.crash.lock().unwrap() = Some(Crash { reason, log: keep_crash_log(&self.work) });
            }
        }
        false
    }
    fn game(&self) -> PathBuf {
        self.work.join("extracted")
    }
    fn exe(&self) -> PathBuf {
        match self.mode {
            Mode::Dev(_) => self.work.join("build").join(PACKAGED_EXE),
            Mode::Packaged => self.work.join(PACKAGED_EXE),
        }
    }
    fn lib(&self) -> PathBuf {
        match self.mode {
            Mode::Dev(_) => self.work.join("build").join("lasgame.dll"),
            Mode::Packaged => self.game().join("lasgame.dll"),
        }
    }
    fn ini(&self) -> PathBuf {
        self.work.join("larecomp.ini")
    }
    fn mods_dir(&self) -> PathBuf {
        self.work.join("mods")
    }
    /// The times of what `--status` reads: the runtime, the game code and its id, and the RomFS the setup copied (the
    /// folder, or the romfs.bin the player picked instead).
    fn stamp(&self) -> [Option<SystemTime>; 5] {
        let id = PathBuf::from(format!("{}.id", self.lib().display()));
        let game = self.game();
        [self.exe(), self.lib(), id, game.join("romfs"), game.join("romfs.bin")].map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
    }
    /// Only the game code is built again when the RomFS set up is a folder: an older version's romfs.bin can't serve
    /// the game's movies, so that install is set up again from the folder picked (its compiled objects are reused).
    fn code_only(&self, asked: bool) -> bool {
        asked && matches!(self.mode, Mode::Packaged) && self.game().join("romfs").is_dir()
    }
    /// The runtime's verdict. It runs a process, so the answer is kept until one of the files it reads changes.
    fn code(&self) -> Code {
        if self.work_problem.is_some() {
            return Code::None;
        }
        let stamp = self.stamp();
        let mut cache = self.code.lock().unwrap();
        if let Some((seen, code)) = &*cache {
            if *seen == stamp {
                return *code;
            }
        }
        let mut c = self.runtime();
        c.arg("--status").stderr(Stdio::null());
        hide_window(&mut c);
        let code = c.output().map(|o| parse_status(&String::from_utf8_lossy(&o.stdout))).unwrap_or(Code::None);
        *cache = Some((stamp, code));
        code
    }
    fn built(&self) -> bool {
        self.code() == Code::Ready
    }
    fn shaders_ready(&self) -> bool {
        fs::read_to_string(self.work.join(SHADER_STAMP)).is_ok_and(|s| s == shader_stamp(self))
    }
    /// The game files the player picked: the junction in developer mode, the remembered folder in packaged mode (or,
    /// once that is forgotten, the install folder's own copy).
    fn game_source(&self) -> Option<PathBuf> {
        let set_up = self.game().join("exefs").is_dir().then(|| self.game());
        match self.mode {
            Mode::Dev(_) => set_up,
            Mode::Packaged => fs::read_to_string(self.work.join(PICKED_FILE)).ok().map(|s| PathBuf::from(s.trim())).or(set_up),
        }
    }
    fn tools_missing(&self) -> Vec<String> {
        let needed = match &self.mode {
            Mode::Dev(dev) => vec![
                ("MSYS2 make", dev.msys.join("usr\\bin\\make.exe")),
                ("MSYS2 clang", dev.msys.join("ucrt64\\bin\\clang.exe")),
                ("Python environment (.venv)", dev.repo.join(".venv\\Scripts\\python.exe")),
            ],
            Mode::Packaged => vec![
                ("the game runtime (ForgottenLandRecomp.exe)", self.exe()),
                ("the game code generator (lasgen)", self.work.join("lasgen\\lasgen.exe")),
                ("the compiler (zig)", self.work.join("zig\\zig.exe")),
                ("the game code headers (gamelib-src)", self.work.join("gamelib-src\\cpu.h")),
                ("the runtime's import library (lasrecomp.dll.a)", self.work.join("lasrecomp.dll.a")),
            ],
        };
        needed.into_iter().filter(|(_, path)| !path.is_file()).map(|(name, _)| name.to_string()).collect()
    }
    /// The runtime, started in the work folder. Packaged mode pins its data root there: without it, a folder named
    /// `extracted` next to the install folder would be taken for a source checkout's.
    fn runtime(&self) -> Command {
        let mut c = Command::new(self.exe());
        c.current_dir(&self.work);
        match &self.mode {
            Mode::Dev(dev) => c.env("PATH", dev.tool_path()),
            Mode::Packaged => c.env("LARECOMP_DATA_ROOT", &self.work),
        };
        c
    }
    // the runtime builds nothing here: it only checks the dump linked as `extracted` holds a RomFS
    fn check_romfs(&self) -> Command {
        let mut c = self.runtime();
        c.args(["extracted", "--build-romfs"]);
        hide_window(&mut c);
        c
    }
    fn settings(&self) -> Vec<(String, String)> {
        let text = fs::read_to_string(self.ini()).unwrap_or_default();
        SETTING_KEYS
            .iter()
            .map(|k| {
                let v = text.lines().find_map(|l| {
                    let (key, val) = l.split_once('=')?;
                    (key.trim() == *k).then(|| val.trim().to_string())
                });
                (k.to_string(), v.unwrap_or_default())
            })
            .collect()
    }
}

impl Dev {
    fn tool_path(&self) -> String {
        let old = std::env::var("PATH").unwrap_or_default();
        format!("{};{};{old}", self.msys.join("ucrt64\\bin").display(), self.msys.join("usr\\bin").display())
    }
    // WORK relative to the repo, on one drive: the link line carries about 540 object paths, which only fit what Windows
    // allows (Argument list too long) while WORK is a short relative path
    fn work_from_repo(&self, work: &Path) -> PathBuf {
        let repo = fs::canonicalize(&self.repo).unwrap_or(self.repo.clone());
        let strip = |p: &Path| PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase());
        let (r, w) = (strip(&repo), strip(work));
        let (rc, wc): (Vec<_>, Vec<_>) = (r.components().collect(), w.components().collect());
        if rc.first() != wc.first() {
            return work.to_path_buf();
        }
        let same = rc.iter().zip(&wc).take_while(|(a, b)| a == b).count();
        let mut rel = PathBuf::new();
        (same..rc.len()).for_each(|_| rel.push(".."));
        work.components().skip(same).for_each(|c| rel.push(c));
        rel
    }
    fn make(&self, work: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(self.msys.join("usr\\bin\\make.exe"));
        // MSYS make wants forward slashes
        let work = self.work_from_repo(work).to_string_lossy().replace('\\', "/");
        c.current_dir(&self.repo).env("PATH", self.tool_path()).env("MSYSTEM", "UCRT64").arg(format!("WORK={work}")).args(args);
        hide_window(&mut c);
        c
    }
}

#[cfg(windows)]
fn hide_window(c: &mut Command) {
    use std::os::windows::process::CommandExt;
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
fn hide_window(_: &mut Command) {}

/// Too little free space for the build, in words. The drive is the work folder's.
fn disk_problem(work: &Path, need: u64) -> Option<String> {
    let free = win::free_bytes(work).filter(|free| *free < need)?;
    let drive: String = work.to_string_lossy().chars().take(2).collect();
    Some(format!(
        "There is not enough free disk space on {drive}. The build needs about {:.1} GB and {:.1} GB are free. Free some space, or move this folder to a drive with more room.",
        need as f64 / 1e9,
        free as f64 / 1e9
    ))
}

/// A build that failed on a full disk reports whatever failed first: say it is the disk.
fn disk_full_note(work: &Path) -> Option<String> {
    win::free_bytes(work).filter(|free| *free < 200_000_000).map(|_| "The disk is full. Free some space and build again.".into())
}

/// Why the build cannot run in this install folder, found once at startup (Windows paths stop at 260 characters and
/// the build adds about 45 below this folder).
fn work_problem(mode: &Mode, work: &Path) -> Option<String> {
    if !matches!(mode, Mode::Packaged) {
        return None;
    }
    let n = work.to_string_lossy().encode_utf16().count();
    if n > MAX_WORK_PATH {
        return Some(format!("The path of this folder is {n} characters long, and Windows stops the build at about {MAX_WORK_PATH}. Move the Forgotten Land Recomp folder to a shorter path, such as C:\\Games\\ForgottenLandRecomp, and start it again."));
    }
    let probe = work.join(".write-test");
    let writable = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    (!writable).then(|| "The launcher cannot write to its folder. Move the Forgotten Land Recomp folder somewhere you can write to, such as Documents, and start it again.".into())
}

type St<'a> = tauri::State<'a, Arc<App>>;

// (async: it may run the runtime, and the window must not wait for that)
#[tauri::command(async)]
fn status(app: St) -> Status {
    let idle = matches!(app.job.lock().unwrap().phase, Phase::Idle | Phase::Failed);
    let code = if idle { app.code() } else { Code::None };
    let (game_dir, game_error) = match app.game_source() {
        Some(game) => {
            let shown = fs::canonicalize(&game).unwrap_or(game.clone());
            // packaged mode asks the runtime, a process: once when the folder is picked and again by the setup itself
            let check = matches!(app.mode, Mode::Dev(_));
            (Some(shown.to_string_lossy().trim_start_matches(r"\\?\").to_string()), check.then(|| check_game(&game).err()).flatten())
        }
        None => (None, None),
    };
    let game_error = game_error.or_else(|| Some(app.pick_error.lock().unwrap().clone()).filter(|e| !e.is_empty()));
    let running = app.poll_game();
    let job = app.job.lock().unwrap();
    let progress = match (&app.mode, job.phase) {
        (Mode::Packaged, _) => job.progress,
        (Mode::Dev(_), Phase::Compile) => {
            let total = count(&app.work.join("generated"), "c").max(1);
            Some(count(&app.work.join("build").join("obj"), "o") as f32 / total as f32)
        }
        _ => None,
    };
    Status {
        work: app.work.to_string_lossy().into(),
        game_dir,
        game_error,
        tools_missing: app.tools_missing(),
        work_error: app.work_problem.clone(),
        built: code == Code::Ready,
        rebuild: code == Code::Build,
        running,
        phase: job.phase,
        progress,
        detail: job.detail.clone(),
        error: job.error.clone(),
        settings: app.settings(),
        mods: read_mods(&app.mods_dir()),
        crash: app.crash.lock().unwrap().clone(),
        packaged: matches!(app.mode, Mode::Packaged),
        shaders_ready: app.shaders_ready(),
        update: app.updater.state(),
        version: env!("CARGO_PKG_VERSION"),
        auto_update: update::pref(&app.work, update::AUTO_KEY, false),
        stable_only: update::pref(&app.work, update::STABLE_KEY, true),
    }
}

// (async: the runtime's search of a big folder must not freeze the window)
#[tauri::command(async)]
fn set_game_dir(app: St, path: String) -> Result<(), String> {
    let result = pick_game_dir(&app, PathBuf::from(&path));
    *app.pick_error.lock().unwrap() = result.as_ref().err().map(|e| format!("{path} was not used. {e}")).unwrap_or_default();
    result
}

fn pick_game_dir(app: &App, src: PathBuf) -> Result<(), String> {
    match app.mode {
        Mode::Dev(_) => check_game(&src)?,
        Mode::Packaged => runtime_check(app, &src)?,
    }
    if let Mode::Packaged = app.mode {
        return fs::write(app.work.join(PICKED_FILE), src.to_string_lossy().as_bytes()).map_err(|e| e.to_string());
    }
    let link = app.game();
    if fs::canonicalize(&link).ok() == fs::canonicalize(&src).ok() {
        return Ok(());
    }
    fs::create_dir_all(&app.work).map_err(|e| e.to_string())?;
    link_game_dir(&link, &src)
}

// A junction made through the Windows API, not `cmd /C mklink`: cmd re-parses `&`, `^` and `%` in the path
fn link_game_dir(link: &Path, src: &Path) -> Result<(), String> {
    // symlink_metadata sees a dead junction (the player moved their dump), which exists() follows and misses
    if fs::symlink_metadata(link).is_ok() {
        // remove_dir only removes a junction or an empty folder: real game files are never deleted here
        fs::remove_dir(link).map_err(|_| format!("{} already holds game files. Move that folder away first.", link.display()))?;
    }
    junction::create(src, link).map_err(|e| e.to_string())
}

// Starts the step as the job's child and hands back its stdout when piped
fn spawn_step(app: &App, phase: Phase, cmd: &mut Command) -> Result<Option<ChildStdout>, String> {
    // the flag check and the registration share one lock hold, so a cancel never lands between spawn and registration
    let mut job = app.job.lock().unwrap();
    if job.cancelled {
        return Err("Cancelled.".into());
    }
    let mut child = cmd.stdin(Stdio::null()).spawn().map_err(|e| format!("{cmd:?}: {e}"))?;
    win::end_with_launcher(&child);
    job.phase = phase;
    let out = child.stdout.take();
    job.child = Some(child);
    Ok(out)
}

fn run_step(app: &App, phase: Phase, mut cmd: Command, log: &mut fs::File) -> Result<(), String> {
    let out = log.try_clone().map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let _ = writeln!(log, "\n== {cmd:?}");
    spawn_step(app, phase, cmd.stdout(out).stderr(err))?;
    wait_step(app)
}

// Packaged mode's Build: the runtime's headless setup, its progress lines mapped onto the phases
fn run_setup(app: &App, first: Phase, mut cmd: Command, log: &mut fs::File) -> Result<(), String> {
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let _ = writeln!(log, "== {cmd:?}");
    let out = spawn_step(app, first, cmd.stdout(Stdio::piped()).stderr(err))?.ok_or("no setup output")?;
    let mut end = None;
    for line in BufReader::new(out).lines().map_while(Result::ok) {
        match parse_setup_line(&line) {
            Some(SetupLine::Progress(phase, fraction, detail)) => {
                let mut job = app.job.lock().unwrap();
                (job.phase, job.progress, job.detail) = (phase, Some(fraction), detail);
            }
            other => end = other.or(end),
        }
    }
    match (wait_step(app), end) {
        (Err(_), Some(SetupLine::Failed(reason))) => Err(setup_failure(&reason)),
        (result, _) => result,
    }
}

fn wait_step(app: &App) -> Result<(), String> {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let mut job = app.job.lock().unwrap();
        let Some(child) = job.child.as_mut() else { return Err("Cancelled.".into()) };
        if let Ok(Some(code)) = child.try_wait() {
            job.child = None;
            return if code.success() { Ok(()) } else { Err("A build step failed. Open the setup log for the details.".into()) };
        }
    }
}

#[tauri::command]
fn start_setup(app: St, code_only: Option<bool>) -> Result<(), String> {
    if app.updater.working() {
        return Err(UPDATING.into());
    }
    // only the game code, from the game files set up already: no folder to pick, none needed
    let code_only = app.code_only(code_only.unwrap_or(false));
    let game = if code_only { None } else { Some(app.game_source().ok_or("Select your game folder first.")?) };
    if let (Mode::Dev(_), Some(game)) = (&app.mode, &game) {
        check_game(game)?;
    }
    if let Some(problem) = &app.work_problem {
        return Err(problem.clone());
    }
    let missing = app.tools_missing();
    if !missing.is_empty() {
        return Err(format!("Missing build tools: {}.", missing.join(", ")));
    }
    if let Some(problem) = disk_problem(&app.work, if code_only { CODE_BYTES } else { BUILD_BYTES }) {
        return Err(problem);
    }
    {
        let mut job = app.job.lock().unwrap();
        if !matches!(job.phase, Phase::Idle | Phase::Failed) {
            return Ok(());
        }
        let phase = match (&app.mode, code_only) {
            (_, true) => Phase::Translate,
            (Mode::Packaged, _) => Phase::Files,
            _ => Phase::Runtime,
        };
        *job = Job { phase, ..Job::default() };
    }
    let app = Arc::clone(&app);
    std::thread::spawn(move || {
        let jobs = format!("-j{}", std::thread::available_parallelism().map_or(4, |n| n.get()));
        let result = fs::File::create(app.work.join("setup.log")).map_err(|e| format!("The launcher cannot write to its folder ({e}). Move the Forgotten Land Recomp folder somewhere you can write to, such as Documents.")).and_then(|mut log| match &app.mode {
            Mode::Dev(dev) => {
                let exe = format!("{}/build/{PACKAGED_EXE}", dev.work_from_repo(&app.work).to_string_lossy().replace('\\', "/"));
                run_step(&app, Phase::Runtime, dev.make(&app.work, &[&jobs, &exe]), &mut log)?;
                run_step(&app, Phase::Files, app.check_romfs(), &mut log)?;
                run_step(&app, Phase::Translate, dev.make(&app.work, &["gen"]), &mut log)?;
                run_step(&app, Phase::Compile, dev.make(&app.work, &[&jobs]), &mut log)
            }
            Mode::Packaged => {
                let mut c = app.runtime();
                match &game {
                    Some(game) => c.arg("--setup").arg(game),
                    None => c.arg("--rebuild-code"),
                };
                hide_window(&mut c);
                run_setup(&app, Phase::Files, c, &mut log)
            }
        }.and_then(|()| run_shaders(&app, &mut log)));
        let mut job = app.job.lock().unwrap();
        match result {
            Ok(()) => job.phase = Phase::Idle,
            Err(e) => {
                job.phase = if job.cancelled { Phase::Idle } else { Phase::Failed };
                job.error = if job.cancelled { String::new() } else { disk_full_note(&app.work).unwrap_or(e) };
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_setup(app: St) {
    cancel(&app);
}

fn cancel(app: &App) {
    let child = {
        let mut job = app.job.lock().unwrap();
        job.cancelled = true;
        job.child.take()
    };
    // killed outside the lock so the status poll does not stall
    if let Some(mut child) = child {
        // make's compiler children too
        let mut k = Command::new("taskkill");
        k.args(["/F", "/T", "/PID", &child.id().to_string()]);
        hide_window(&mut k);
        let _ = k.status();
        let _ = child.wait();
    }
}

#[tauri::command]
fn play(app: St) -> Result<(), String> {
    if app.updater.working() {
        return Err(UPDATING.into());
    }
    let mut game = app.game.lock().unwrap();
    if game.as_mut().is_some_and(|(c, _)| matches!(c.try_wait(), Ok(None))) {
        return Ok(());
    }
    if !app.built() {
        return Err("The game code was not built by this version of the program. Open Set up and build it again.".into());
    }
    *app.crash.lock().unwrap() = None;
    if app.shaders_ready() {
        return start_game(&app, &mut game);
    }
    drop(game);
    {
        let mut job = app.job.lock().unwrap();
        if !matches!(job.phase, Phase::Idle | Phase::Failed) {
            return Ok(());
        }
        *job = Job { phase: Phase::Shaders, ..Job::default() };
    }
    let app = Arc::clone(&app);
    std::thread::spawn(move || {
        let result = fs::File::create(app.work.join("setup.log"))
            .map_err(|e| e.to_string())
            .and_then(|mut log| run_shaders(&app, &mut log))
            .and_then(|()| start_game(&app, &mut app.game.lock().unwrap()));
        let mut job = app.job.lock().unwrap();
        match result {
            Ok(()) => job.phase = Phase::Idle,
            Err(e) => {
                job.phase = if job.cancelled { Phase::Idle } else { Phase::Failed };
                job.error = if job.cancelled { String::new() } else { e };
            }
        }
    });
    Ok(())
}

fn start_game(app: &App, game: &mut Option<(Child, Instant)>) -> Result<(), String> {
    let log = fs::File::create(app.work.join("game.log")).map_err(|e| e.to_string())?;
    let child = app
        .runtime()
        .args(["extracted", "--play"])
        .env("LARECOMP_GAME_LIB", app.lib())
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .spawn()
        .map_err(|e| format!("Could not start the game: {e}"))?;
    *game = Some((child, Instant::now()));
    Ok(())
}

/// What the prepared shaders were made for: the runtime (its shader translator), the display drivers (each compiles
/// its own) and the mods switched on (they can replace shaders). Another one prepares them again before play.
fn shader_stamp(app: &App) -> String {
    let exe = fs::metadata(app.exe()).ok().map(|m| (m.len(), m.modified().ok()));
    let mods: Vec<String> = read_mods(&app.mods_dir()).into_iter().filter(|m| m.on).map(|m| m.name).collect();
    format!("runtime {exe:?}\ndrivers {}\nmods {}\n", display_drivers(), mods.join("/"))
}

/// The display adapters' driver versions (read once per launcher start)
#[cfg(windows)]
fn display_drivers() -> String {
    static SEEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SEEN.get_or_init(|| {
        let mut c = Command::new("reg");
        c.args(["query", r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}", "/s", "/v", "DriverVersion"]);
        hide_window(&mut c);
        let out = c.output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        out.lines().filter(|l| l.contains("DriverVersion")).filter_map(|l| l.split_whitespace().last()).collect::<Vec<_>>().join(",")
    })
    .clone()
}
#[cfg(not(windows))]
fn display_drivers() -> String {
    String::new()
}

/// Every shader of the game files prepared (minutes, once): the game started hidden, prints its progress, and exits
fn run_shaders(app: &App, log: &mut fs::File) -> Result<(), String> {
    let mut c = app.runtime();
    c.args(["extracted", "--play"]).env("LARECOMP_GAME_LIB", app.lib()).stdin(Stdio::null());
    for v in ["LARECOMP_HIDDEN", "LARECOMP_NO_USER_INPUT", "LARECOMP_SHADER_PREP", "LARECOMP_SHADER_PREBUILD", "LARECOMP_SHADER_PREP_EXIT"] {
        c.env(v, "1");
    }
    hide_window(&mut c);
    run_setup(app, Phase::Shaders, c, log)?;
    fs::write(app.work.join(SHADER_STAMP), shader_stamp(app)).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_setting(app: St, key: String, value: String) -> Result<(), String> {
    if !SETTING_KEYS.contains(&key.as_str()) || value.contains(['\n', '\r']) {
        return Err("Unknown setting.".into());
    }
    let text = fs::read_to_string(app.ini()).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().filter(|l| l.split_once('=').map(|(k, _)| k.trim()) != Some(key.as_str())).map(String::from).collect();
    lines.push(format!("{key} = {value}"));
    fs::create_dir_all(&app.work).map_err(|e| e.to_string())?;
    fs::write(app.ini(), lines.join("\n") + "\n").map_err(|e| e.to_string())
}

#[derive(Debug, PartialEq, Serialize)]
struct ModEntry {
    name: String,
    on: bool,
}

/// The mods in `dir` in the order the game applies them, switched off ones in place. This must stay the rule of
/// runtime/mods.c: a folder holding romfs/ is a mod, mods.txt lists them first (`-name` is off, the first mention wins,
/// unknown names are ignored), the rest follow in name order, on.
fn read_mods(dir: &Path) -> Vec<ModEntry> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("romfs").is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && n.len() < 256)
        .collect();
    found.sort();
    let listed = String::from_utf8_lossy(&fs::read(dir.join("mods.txt")).unwrap_or_default()).into_owned();
    let mut mods: Vec<ModEntry> = Vec::new();
    for line in listed.lines() {
        let (name, on) = line.strip_prefix('-').map_or((line, true), |n| (n, false));
        if found.iter().any(|f| f == name) && !mods.iter().any(|m| m.name == name) {
            mods.push(ModEntry { name: name.into(), on });
        }
    }
    for name in found {
        if !mods.iter().any(|m| m.name == name) {
            mods.push(ModEntry { name, on: true });
        }
    }
    mods
}

/// The whole order, every time: written beside the file and renamed over it, so the game never reads half of it. A name
/// starting with a dash cannot be listed as on (that line would read as off), so it is left out and stays unlisted.
fn write_mods(dir: &Path, mods: &[ModEntry]) -> Result<(), String> {
    let text: String = mods
        .iter()
        .filter_map(|m| match (m.on, m.name.starts_with('-')) {
            (false, _) => Some(format!("-{}\n", m.name)),
            (true, false) => Some(format!("{}\n", m.name)),
            (true, true) => None,
        })
        .collect();
    let tmp = dir.join("mods.txt.tmp");
    fs::write(&tmp, text).and_then(|_| fs::rename(&tmp, dir.join("mods.txt"))).map_err(|e| e.to_string())
}

fn edit_mods(dir: &Path, name: &str, change: impl FnOnce(&mut Vec<ModEntry>, usize)) -> Result<(), String> {
    let mut mods = read_mods(dir);
    let i = mods.iter().position(|m| m.name == name).ok_or("That mod is no longer in the mods folder.")?;
    change(&mut mods, i);
    write_mods(dir, &mods)
}

#[tauri::command]
fn set_mod(app: St, name: String, on: bool) -> Result<(), String> {
    edit_mods(&app.mods_dir(), &name, |mods, i| mods[i].on = on)
}

#[tauri::command]
fn move_mod(app: St, name: String, up: bool) -> Result<(), String> {
    edit_mods(&app.mods_dir(), &name, |mods, i| {
        let j = if up { i.wrapping_sub(1) } else { i + 1 };
        if j < mods.len() {
            mods.swap(i, j);
        }
    })
}

const UPDATING: &str = "An update is being installed. The launcher restarts when it is done.";

/// Why an update must wait, if it must: it replaces the runtime and the tools a running game or build uses.
fn update_blocked(app: &App) -> Option<String> {
    if app.poll_game() {
        return Some("Close the game before updating.".into());
    }
    let busy = !matches!(app.job.lock().unwrap().phase, Phase::Idle | Phase::Failed);
    busy.then(|| "Wait for the build or the shader preparation to finish before updating.".into())
}

/// Downloads and installs the update that was found, then starts the new launcher in place of this one.
fn begin_update(app: &Arc<App>, handle: &tauri::AppHandle) -> Result<(), String> {
    if !matches!(app.mode, Mode::Packaged) {
        return Err("Updates are for the release folder.".into());
    }
    if let Some(why) = update_blocked(app) {
        return Err(why);
    }
    let update::Update::Available(release) = app.updater.state() else { return Err("There is no update to install.".into()) };
    let (app, handle) = (Arc::clone(app), handle.clone());
    std::thread::spawn(move || match update::apply(&app.updater, &app.work, &release) {
        Ok(()) => match Command::new(app.work.join(LAUNCHER_EXE)).current_dir(&app.work).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            Ok(_) => handle.exit(0),
            Err(e) => update::fail(&app.updater, format!("The update is installed, but the launcher could not start again ({e}). Start it from this folder.")),
        },
        Err(_) => {} // apply() put the reason in the state
    });
    Ok(())
}

#[tauri::command]
fn start_update(app: St, handle: tauri::AppHandle) -> Result<(), String> {
    begin_update(&app, &handle)
}

#[tauri::command]
fn cancel_update(app: St) {
    app.updater.cancel();
}

#[tauri::command(async)]
fn check_update(app: St) {
    if matches!(app.mode, Mode::Packaged) {
        update::check(&app.updater, &app.work, true);
    }
}

#[tauri::command]
fn set_pref(app: St, key: String, on: bool) -> Result<(), String> {
    if ![update::AUTO_KEY, update::STABLE_KEY].contains(&key.as_str()) {
        return Err("Unknown preference.".into());
    }
    update::set_pref(&app.work, &key, on)?;
    if key == update::STABLE_KEY && matches!(app.mode, Mode::Packaged) {
        // the channel changed: what is offered changes with it
        let app = Arc::clone(&app);
        std::thread::spawn(move || update::check(&app.updater, &app.work, true));
    }
    Ok(())
}

/// Every start: delete what the last update left, look for a newer version, and install it when the player asked for that.
fn update_on_start(app: Arc<App>, handle: tauri::AppHandle) {
    update::cleanup(&app.work);
    update::check(&app.updater, &app.work, false);
    if update::pref(&app.work, update::AUTO_KEY, false) && matches!(app.updater.state(), update::Update::Available(_)) {
        let _ = begin_update(&app, &handle);
    }
}

#[tauri::command]
fn dismiss_crash(app: St) {
    *app.crash.lock().unwrap() = None;
}

#[tauri::command]
fn open_path(app: St, which: String) -> Result<(), String> {
    let p = match which.as_str() {
        "work" => app.work.clone(),
        "mods" => {
            fs::create_dir_all(app.mods_dir()).map_err(|e| e.to_string())?;
            app.mods_dir()
        }
        "setup_log" => app.work.join("setup.log"),
        "game_log" => app.work.join("game.log"),
        // the file name is the one the backend recorded, never a path from the page
        "crash_log" => match &*app.crash.lock().unwrap() {
            Some(c) => app.work.join(&c.log),
            None => return Err("There is no crash log.".into()),
        },
        _ => return Err("Unknown path.".into()),
    };
    if !p.exists() {
        return Err(format!("{} does not exist yet.", p.display()));
    }
    Command::new("explorer").arg(p).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn main() {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
    let mode = Mode::detect(&exe_dir);
    let work = match &mode {
        Mode::Dev(dev) => std::env::var("KFL_WORK").map(PathBuf::from).unwrap_or_else(|_| dev.repo.join("..").join("work").join("kfl")),
        Mode::Packaged => exe_dir,
    };
    let work = fs::canonicalize(&work).map(|p| PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\"))).unwrap_or(work);
    let app = Arc::new(App {
        work_problem: work_problem(&mode, &work),
        work,
        mode,
        job: Arc::new(Mutex::new(Job::default())),
        game: Mutex::new(None),
        crash: Mutex::new(None),
        pick_error: Mutex::new(String::new()),
        code: Mutex::new(None),
        updater: update::Updater::default(),
    });
    let on_start = Arc::clone(&app);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app)
        .setup(move |t| {
            if matches!(on_start.mode, Mode::Packaged) {
                let handle = t.handle().clone();
                std::thread::spawn(move || update_on_start(on_start, handle));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![status, set_game_dir, start_setup, cancel_setup, play, set_setting, set_mod, move_mod, open_path, dismiss_crash, start_update, cancel_update, check_update, set_pref])
        .run(tauri::generate_context!())
        .expect("launcher failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(repo: &str) -> Dev {
        Dev { repo: repo.into(), msys: PathBuf::new() }
    }

    fn app(repo: &str, work: &str) -> App {
        App {
            mode: Mode::Dev(dev(repo)),
            work: work.into(),
            work_problem: None,
            job: Arc::new(Mutex::new(Job::default())),
            game: Mutex::new(None),
            crash: Mutex::new(None),
            pick_error: Mutex::new(String::new()),
            code: Mutex::new(None),
            updater: update::Updater::default(),
        }
    }

    #[test]
    fn work_from_repo_is_relative_on_one_drive() {
        assert_eq!(dev(r"E:\a\repo").work_from_repo(Path::new(r"E:\a\kfl-e2e")), PathBuf::from(r"..\kfl-e2e"));
        assert_eq!(dev(r"E:\a\repo").work_from_repo(Path::new(r"E:\b\w")), PathBuf::from(r"..\..\b\w"));
        assert_eq!(dev(r"E:\a\repo").work_from_repo(Path::new(r"D:\w")), PathBuf::from(r"D:\w"));
    }

    #[test]
    fn the_bundled_tools_select_packaged_mode() {
        let root = scratch("mode");
        assert!(matches!(Mode::detect(&root), Mode::Packaged), "no repo and no tools: still a release folder");
        fs::create_dir_all(root.join("runtime")).unwrap();
        fs::write(root.join("Makefile"), "").unwrap();
        assert!(matches!(Mode::detect(&root), Mode::Dev(_)));
        for tool in ["lasgen\\lasgen.exe", "zig\\zig.exe"] {
            fs::create_dir_all(root.join(tool).parent().unwrap()).unwrap();
            fs::write(root.join(tool), "").unwrap();
        }
        assert!(matches!(Mode::detect(&root), Mode::Packaged));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn setup_lines_map_onto_the_phases() {
        assert_eq!(parse_setup_line("phase looking 0.000"), Some(SetupLine::Progress(Phase::Files, 0.0, None)));
        assert_eq!(parse_setup_line("phase copying 0.010"), Some(SetupLine::Progress(Phase::Files, 0.01, None)));
        assert_eq!(parse_setup_line("phase reading 0.200"), Some(SetupLine::Progress(Phase::Translate, 0.2, None)));
        assert_eq!(parse_setup_line("phase finishing 0.990"), Some(SetupLine::Progress(Phase::Compile, 0.99, None)));
        assert_eq!(parse_setup_line("phase shaders 0.250 5000/20000"), Some(SetupLine::Progress(Phase::Shaders, 0.25, Some("5000/20000".into()))));
        assert_eq!(
            parse_setup_line("phase compiling 0.925 (main_180.c, 7 min)"),
            Some(SetupLine::Progress(Phase::Compile, 0.925, Some("(main_180.c, 7 min)".into())))
        );
        assert_eq!(parse_setup_line("done"), Some(SetupLine::Done));
        assert_eq!(parse_setup_line("failed version"), Some(SetupLine::Failed("version".into())));
        assert_eq!(parse_setup_line("[paths] Forgotten Land Recomp 0.1.0"), None);
        assert_eq!(parse_setup_line("phase someday 0.5"), None);
        assert_eq!(parse_setup_line("phase image 0.100"), None, "the runtime builds no image: it has no such phase");
    }

    #[test]
    fn the_runtime_check_decides_the_folder() {
        assert_eq!(check_verdict("ok\r\n"), Ok(()));
        assert_eq!(check_verdict("[paths] Forgotten Land Recomp 0.2.0\nfailed not_found\n"), Err(setup_failure("not_found")));
        assert_eq!(check_verdict("failed version\n"), Err(setup_failure("version")));
        assert!(check_verdict("").is_err());
        assert!(setup_failure("not_found").contains("msg and gfx"));
    }

    #[test]
    fn a_failed_setup_reports_its_reason() {
        let root = scratch("setupfail");
        let app = app("", "");
        let mut c = Command::new("cmd");
        c.args(["/C", "echo phase compiling 0.500& echo failed copy& exit 1"]);
        let r = run_setup(&app, Phase::Files, c, &mut fs::File::create(root.join("log")).unwrap());
        assert_eq!(r, Err(setup_failure("copy")));
        assert_eq!(app.job.lock().unwrap().progress, Some(0.5));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_runtimes_status_line_decides_the_game_code() {
        assert_eq!(parse_status("ready\r\n"), Code::Ready);
        assert_eq!(parse_status("[paths] x\nbuild\n"), Code::Build);
        assert_eq!(parse_status("none\n"), Code::None);
        assert_eq!(parse_status(""), Code::None);
        assert_eq!(parse_status("[vk] ready to render\n"), Code::None);
    }

    #[test]
    fn the_failure_texts_say_what_the_runtime_refused() {
        let text = setup_failure("version");
        assert!(text.contains("another game") && text.contains("version of Kirby") && text.contains("1.0.0"), "{text}");
        assert!(!text.contains("That is not Kirby"), "{text}");
        let text = setup_failure("bad_image");
        assert!(text.contains("romfs.bin") && text.contains("romfs folder") && !text.contains("game data image"), "{text}");
    }

    #[test]
    fn the_status_is_asked_again_when_the_romfs_folder_or_image_changes() {
        let root = scratch("stamp");
        let app = app("", root.to_str().unwrap());
        let none = app.stamp();
        fs::create_dir_all(root.join("extracted").join("romfs")).unwrap();
        let folder = app.stamp();
        assert_ne!(folder, none, "the setup's RomFS folder is what --status reads");
        fs::write(root.join("extracted").join("romfs.bin"), "x").unwrap();
        assert_ne!(app.stamp(), folder, "so is a romfs.bin the player picked");
        fs::remove_dir(root.join("extracted").join("romfs")).unwrap();
        fs::remove_file(root.join("extracted").join("romfs.bin")).unwrap();
        assert_eq!(app.stamp(), none);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_older_versions_romfs_image_is_set_up_again_not_only_rebuilt() {
        let root = scratch("codeonly");
        let mut app = app("", root.to_str().unwrap());
        app.mode = Mode::Packaged;
        fs::create_dir_all(root.join("extracted")).unwrap();
        fs::write(root.join("extracted").join("romfs.bin"), "x").unwrap();
        assert!(!app.code_only(true), "0.2.0's romfs.bin: the movies need the folder");
        fs::create_dir_all(root.join("extracted").join("romfs")).unwrap();
        assert!(app.code_only(true));
        assert!(!app.code_only(false));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_dump_is_checked_the_way_the_setup_checks_it() {
        let root = scratch("dump");
        let npdm = |id: u64| {
            let mut b = vec![0u8; 0x100];
            b[..4].copy_from_slice(b"META");
            b[0x70..0x74].copy_from_slice(&0x80u32.to_le_bytes());
            b[0x80..0x84].copy_from_slice(b"ACI0");
            b[0x90..0x98].copy_from_slice(&id.to_le_bytes());
            b
        };
        let msg = |r: Result<(), String>| r.unwrap_err();
        assert!(msg(check_game(&root)).contains("no exefs\\main"));
        fs::create_dir_all(root.join("exefs")).unwrap();
        for m in MODULES {
            fs::write(root.join("exefs").join(m), "x").unwrap();
        }
        assert!(msg(check_game(&root)).contains("no romfs\\msg"));
        fs::create_dir_all(root.join("romfs").join("msg")).unwrap();
        fs::create_dir_all(root.join("romfs").join("gfx")).unwrap();
        assert!(msg(check_game(&root)).contains("main.npdm is missing"));
        fs::write(root.join("exefs").join("main.npdm"), npdm(0x0100_4D30_0C5A_E800)).unwrap();
        assert_eq!(msg(check_game(&root)), setup_failure("update"));
        assert!(setup_failure("update").contains("base game"));
        fs::write(root.join("exefs").join("main.npdm"), npdm(0x0100_0000_0000_1234)).unwrap();
        assert!(msg(check_game(&root)).contains("another game") && msg(check_game(&root)).contains("0100000000001234"));
        fs::write(root.join("exefs").join("main.npdm"), npdm(PROGRAM_ID)).unwrap();
        assert_eq!(check_game(&root), Ok(()));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_vulkan_driver_says_what_to_do() {
        let log = format!("{START}FATAL: SDL_CreateWindow: Installed Vulkan doesn't implement the VK_KHR_surface extension\nhost backtrace:\n");
        let Outcome::Crashed(reason) = classify(3, &log) else { panic!("a crash") };
        assert!(explain(reason).starts_with("Your graphics card or its driver may not support Vulkan 1.3. Update the graphics driver"));
        assert_eq!(explain("FATAL: out of memory".into()), "FATAL: out of memory");
    }

    #[test]
    fn too_little_disk_space_is_said_before_the_build_starts() {
        let root = scratch("disk");
        let msg = disk_problem(&root, u64::MAX / 2).expect("no drive has that much room");
        assert!(msg.contains("not enough free disk space") && msg.contains("move this folder"), "{msg}");
        assert_eq!(disk_problem(&root, 1), None);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_long_install_path_is_refused_up_front() {
        let long = PathBuf::from(format!("C:\\{}", "a".repeat(MAX_WORK_PATH)));
        assert!(work_problem(&Mode::Packaged, &long).unwrap().contains("shorter path"));
        assert_eq!(work_problem(&Mode::Dev(dev("")), &long), None);
        let root = scratch("workok");
        assert_eq!(work_problem(&Mode::Packaged, &root), None);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_launcher_ending_ends_what_it_started() {
        let job = win::kill_on_close_job();
        let mut child = sleeper().stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        assert!(win::join(job, &child));
        assert!(child.try_wait().unwrap().is_none());
        // closing the job is what Windows does when the launcher ends, however it ends
        unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        let started = Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(started.elapsed().as_secs() < 5, "the child outlived its job");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn a_game_that_closes_at_once_is_reported() {
        let root = scratch("early");
        let mut app = app("", root.to_str().unwrap());
        app.work = root.clone();
        fs::write(root.join("game.log"), START).unwrap();
        let mut c = Command::new("cmd");
        c.args(["/C", "exit 1"]);
        *app.game.lock().unwrap() = Some((c.spawn().unwrap(), Instant::now()));
        while app.poll_game() {
            std::thread::sleep(Duration::from_millis(50));
        }
        let crash = app.crash.lock().unwrap().clone().expect("a card for a game that closed at once");
        assert!(crash.reason.contains("closed 0 seconds after it started (exit code 1)"), "{}", crash.reason);
        assert!(root.join(&crash.log).is_file());
        fs::remove_dir_all(&root).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kfl-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn junction_survives_shell_metacharacters() {
        let root = scratch("meta");
        let src = root.join("D&Kirby^Co %PATH% (a b) 100%");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("probe.txt"), "ok").unwrap();
        let link = root.join("extracted");
        link_game_dir(&link, &src).unwrap();
        assert_eq!(fs::read_to_string(link.join("probe.txt")).unwrap(), "ok");
        assert_eq!(fs::canonicalize(&link).unwrap(), fs::canonicalize(&src).unwrap());
        assert!(!root.join("Co").exists() && !root.join("PATH").exists());
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dead_junction_is_replaced_and_real_folder_is_kept() {
        let root = scratch("dead");
        let (old, new, link) = (root.join("old"), root.join("new"), root.join("extracted"));
        fs::create_dir(&old).unwrap();
        fs::create_dir(&new).unwrap();
        link_game_dir(&link, &old).unwrap();
        fs::remove_dir(&old).unwrap();
        assert!(!link.exists() && fs::symlink_metadata(&link).is_ok());
        link_game_dir(&link, &new).unwrap();
        assert_eq!(fs::canonicalize(&link).unwrap(), fs::canonicalize(&new).unwrap());
        fs::remove_dir(&link).unwrap();
        fs::create_dir(&link).unwrap();
        fs::write(link.join("keep.bin"), "x").unwrap();
        assert!(link_game_dir(&link, &new).is_err());
        assert!(link.join("keep.bin").is_file());
        fs::remove_dir_all(&root).unwrap();
    }

    const START: &str = "[paths] Forgotten Land Recomp 0.1.0, data: E:\\work\\kfl\n[gamelib] E:\\work\\kfl\\build\\lasgame.dll: main, 24961 functions\n[vk] t=2.1 frame 120 60.1 fps\n";

    #[test]
    fn a_fatal_report_is_a_crash() {
        let log = format!("{START}FATAL: LARECOMP_GAME_LIB: no file E:\\work\\kfl\\build\\lasgame.dll\nhost backtrace (guest functions are f_<addr>):\n  #0  00007ff6288c8208 lasrecomp+68208\n");
        assert_eq!(classify(3, &log), Outcome::Crashed("FATAL: LARECOMP_GAME_LIB: no file E:\\work\\kfl\\build\\lasgame.dll".into()));
    }

    #[test]
    fn an_exception_report_is_a_crash() {
        let log = format!("{START}\n*** exception c0000005 accessing 0xffffffffffffffff\n    host pc 7ff6288c8208 (?+0)\n    host lr 7ff6288617b8 (?)\nhost backtrace (guest functions are f_<addr>):\n  #0  00007ff6288c8208 lasrecomp+68208\nguest: sp=723fffe910 lr=700051f500\n");
        assert_eq!(classify(3, &log), Outcome::Crashed("*** exception c0000005 accessing 0xffffffffffffffff".into()));
    }

    #[test]
    fn the_marker_decides_even_when_the_exit_code_looks_normal() {
        assert!(matches!(classify(0, "FATAL: out of memory\n"), Outcome::Crashed(_)));
    }

    #[test]
    fn closing_the_window_is_a_normal_quit() {
        assert_eq!(classify(0, &format!("{START}window closed\n")), Outcome::Quit);
    }

    #[test]
    fn a_kill_without_a_report_is_not_a_crash() {
        assert_eq!(classify(1, START), Outcome::Stopped);
        assert_eq!(classify(-1073741510, START), Outcome::Crashed("Windows ended the game with error C000013A.".into()));
    }

    #[test]
    fn an_abort_without_a_report_is_a_crash() {
        assert!(matches!(classify(3, START), Outcome::Crashed(_)));
    }

    #[test]
    fn only_the_newest_crash_logs_are_kept() {
        let root = scratch("crashlogs");
        for i in 1..=7 {
            fs::write(root.join(format!("crash-20200101-00000{i}.log")), "old").unwrap();
        }
        for other in ["crash-notes.log", "crash-20200101-00000.log", "setup.log", "game.log"] {
            fs::write(root.join(other), "x").unwrap();
        }
        let name = keep_crash_log(&root);
        assert!(is_crash_log(&name) && fs::read_to_string(root.join(&name)).unwrap() == "x");
        let mut left: Vec<String> = fs::read_dir(&root).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).filter(|n| is_crash_log(n)).collect();
        left.sort();
        assert_eq!(left, ["crash-20200101-000004.log", "crash-20200101-000005.log", "crash-20200101-000006.log", "crash-20200101-000007.log", name.as_str()]);
        assert!(root.join("crash-notes.log").is_file() && root.join("setup.log").is_file());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_tail_of_a_huge_log_is_read() {
        let root = scratch("tail");
        fs::write(root.join("game.log"), "x".repeat(300_000) + "\nFATAL: late\n").unwrap();
        assert!(log_tail(&root.join("game.log")).ends_with("FATAL: late\n"));
        assert!(log_tail(&root.join("game.log")).len() <= 1 << 16);
        assert_eq!(log_tail(&root.join("nope")), "");
        fs::remove_dir_all(&root).unwrap();
    }

    fn sleeper() -> Command {
        let mut c = Command::new("ping");
        c.args(["-n", "30", "127.0.0.1"]);
        c
    }

    #[test]
    fn cancel_before_spawn_starts_nothing() {
        let root = scratch("precancel");
        let app = app("", "");
        cancel(&app);
        let mut log = fs::File::create(root.join("log")).unwrap();
        let mut c = Command::new("cmd");
        c.args(["/C", "set /a 40+2"]);
        assert_eq!(run_step(&app, Phase::Runtime, c, &mut log), Err("Cancelled.".into()));
        std::thread::sleep(std::time::Duration::from_millis(1000));
        // a spawned step would have printed 42 into the log
        assert!(!fs::read_to_string(root.join("log")).unwrap().contains("42"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cancel_kills_the_running_step() {
        let root = scratch("cancel");
        let app = Arc::new(app("", ""));
        let (a, log) = (Arc::clone(&app), root.join("log"));
        let t = std::thread::spawn(move || run_step(&a, Phase::Runtime, sleeper(), &mut fs::File::create(log).unwrap()));
        std::thread::sleep(std::time::Duration::from_millis(600));
        assert!(app.job.lock().unwrap().child.is_some());
        let started = std::time::Instant::now();
        cancel(&app);
        assert_eq!(t.join().unwrap(), Err("Cancelled.".into()));
        assert!(started.elapsed().as_secs() < 5 && app.job.lock().unwrap().child.is_none());
        fs::remove_dir_all(&root).unwrap();
    }

    fn mods_in(name: &str, mods: &[&str], file: Option<&str>) -> PathBuf {
        let dir = scratch(name);
        for m in mods {
            fs::create_dir_all(dir.join(m).join("romfs").join("basil")).unwrap();
        }
        if let Some(text) = file {
            fs::write(dir.join("mods.txt"), text).unwrap();
        }
        dir
    }

    fn listed(dir: &Path) -> Vec<(String, bool)> {
        read_mods(dir).into_iter().map(|m| (m.name, m.on)).collect()
    }

    fn pairs(v: &[(&str, bool)]) -> Vec<(String, bool)> {
        v.iter().map(|(n, on)| (n.to_string(), *on)).collect()
    }

    #[test]
    fn mods_follow_mods_txt_then_name_order() {
        let dir = mods_in("mods-order", &["alpha", "beta", "gamma", "delta"], Some("gamma\r\n-alpha\n"));
        assert_eq!(listed(&dir), pairs(&[("gamma", true), ("alpha", false), ("beta", true), ("delta", true)]));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_and_repeated_names_in_mods_txt_are_ignored() {
        let dir = mods_in("mods-junk", &["a", "b"], Some("ghost\nb\n-b\nb\n\n-ghost\n-a\na\n"));
        assert_eq!(listed(&dir), pairs(&[("b", true), ("a", false)]));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_without_romfs_is_not_a_mod() {
        let dir = mods_in("mods-kinds", &["real", ".hidden"], None);
        fs::create_dir_all(dir.join("notes").join("exefs")).unwrap();
        fs::create_dir_all(dir.join("file-romfs")).unwrap();
        fs::write(dir.join("file-romfs").join("romfs"), "").unwrap();
        assert_eq!(listed(&dir), pairs(&[("real", true)]));
        assert!(read_mods(&dir.join("missing")).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn switching_and_moving_mods_is_written_in_full_and_read_back() {
        let dir = mods_in("mods-write", &["a", "b", "c"], None);
        edit_mods(&dir, "a", |m, i| m.swap(i, i + 1)).unwrap();
        edit_mods(&dir, "c", |m, i| m[i].on = false).unwrap();
        assert_eq!(fs::read_to_string(dir.join("mods.txt")).unwrap(), "b\na\n-c\n");
        assert!(!dir.join("mods.txt.tmp").exists());
        assert_eq!(listed(&dir), pairs(&[("b", true), ("a", true), ("c", false)]));
        assert_eq!(edit_mods(&dir, "gone", |_, _| {}), Err("That mod is no longer in the mods folder.".into()));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_mod_dropped_into_a_written_list_comes_after_it_and_a_dash_name_is_not_listed_as_on() {
        let dir = mods_in("mods-new", &["b", "a"], None);
        edit_mods(&dir, "b", |m, i| m[i].on = true).unwrap();
        fs::create_dir_all(dir.join("-x").join("romfs")).unwrap();
        fs::create_dir_all(dir.join("c").join("romfs")).unwrap();
        assert_eq!(listed(&dir), pairs(&[("a", true), ("b", true), ("-x", true), ("c", true)]));
        edit_mods(&dir, "-x", |m, i| m[i].on = false).unwrap();
        edit_mods(&dir, "c", |m, i| m[i].on = true).unwrap();
        assert_eq!(fs::read_to_string(dir.join("mods.txt")).unwrap(), "a\nb\n--x\nc\n");
        edit_mods(&dir, "-x", |m, i| m[i].on = true).unwrap();
        assert_eq!(fs::read_to_string(dir.join("mods.txt")).unwrap(), "a\nb\nc\n");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_update_waits_for_the_game_and_for_a_build() {
        let app = app("", "");
        assert_eq!(update_blocked(&app), None);
        app.job.lock().unwrap().phase = Phase::Shaders;
        assert!(update_blocked(&app).unwrap().contains("shader"));
        app.job.lock().unwrap().phase = Phase::Failed;
        assert_eq!(update_blocked(&app), None, "a failed build does not hold an update back");
        let mut c = Command::new("cmd");
        c.args(["/C", "ping -n 30 127.0.0.1 >nul"]);
        let child = c.spawn().unwrap();
        *app.game.lock().unwrap() = Some((child, Instant::now()));
        assert!(update_blocked(&app).unwrap().contains("game"));
        let (mut child, _) = app.game.lock().unwrap().take().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}
