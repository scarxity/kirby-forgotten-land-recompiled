// The launcher's own updates: the release zip from this project's GitHub releases, checked against the SHA-256 GitHub
// publishes for the asset, and swapped in over the program's own files. Nothing about the player is sent: a plain GET.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const REPO: &str = "scarxity/kirby-forgotten-land-recompiled";
const DEFAULT_API: &str = "https://api.github.com";
/// Overrides the API's base URL, so the updater can be tried against a local server.
const API_ENV: &str = "KFL_UPDATE_API";
/// Launcher-owned preferences, flat keys, beside the exe (larecomp.ini belongs to the game).
const PREFS_FILE: &str = "launcher-prefs.json";
pub const AUTO_KEY: &str = "auto_update";
pub const STABLE_KEY: &str = "stable_only";
/// Where an update is unpacked and the replaced files wait until the next start deletes them.
const STAGING: &str = ".update";
/// What an update never writes, even if a zip carried it: the player's data (everything else a run writes is a log).
const PROTECTED: [&str; 9] = ["extracted", "cache", "mods", "save", "larecomp.ini", "game-folder.txt", PREFS_FILE, "shaders-prepared.txt", STAGING];
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// X.Y.Z or X.Y.Z-beta.N: the only forms a release of this project has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    core: (u32, u32, u32),
    /// A beta is (0, N), the stable release (1, 0): beta.1 < beta.2 < stable.
    stage: (u8, u32),
}

impl Version {
    pub fn parse(tag: &str) -> Option<Version> {
        let tag = tag.strip_prefix('v').unwrap_or(tag);
        let (core, pre) = match tag.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (tag, None),
        };
        let mut n = core.split('.').map(|p| p.parse::<u32>().ok());
        let core = (n.next()??, n.next()??, n.next()??);
        if n.next().is_some() {
            return None;
        }
        let stage = match pre {
            None => (1, 0),
            Some(pre) => (0, pre.strip_prefix("beta.")?.parse().ok()?),
        };
        Some(Version { core, stage })
    }
    fn is_beta(&self) -> bool {
        self.stage.0 == 0
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.core.0, self.core.1, self.core.2)?;
        if self.is_beta() {
            write!(f, "-beta.{}", self.stage.1)?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct GhRelease {
    tag_name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Release {
    version: String,
    notes: String,
    asset: String,
    /// SHA-256 as lower-case hex
    digest: String,
}

/// The newest release that is newer than `current`, on the channel the player chose, with an asset whose digest can be
/// checked. A release without a digest is not offered: it could not be verified.
pub fn pick_release(releases: &[GhRelease], current: &Version, stable_only: bool) -> Option<Release> {
    releases
        .iter()
        .filter(|r| !r.draft && !(stable_only && r.prerelease))
        .filter_map(|r| Some((Version::parse(&r.tag_name)?, r)))
        .filter(|(v, _)| *v > *current && !(stable_only && v.is_beta()))
        .filter_map(|(v, r)| {
            let name = format!("ForgottenLandRecomp-{v}-Windows-x64.zip");
            let asset = r.assets.iter().find(|a| a.name == name)?;
            let digest = asset.digest.as_deref()?.strip_prefix("sha256:")?.to_ascii_lowercase();
            Some((v, Release { version: v.to_string(), notes: r.body.clone().unwrap_or_default(), asset: asset.browser_download_url.clone(), digest }))
        })
        .max_by_key(|(v, _)| *v)
        .map(|(_, release)| release)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Update {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading { progress: f32 },
    Installing,
    Failed { msg: String },
}

#[derive(Default)]
pub struct Updater {
    state: Mutex<Update>,
    cancel: AtomicBool,
}

impl Updater {
    pub fn state(&self) -> Update {
        self.state.lock().unwrap().clone()
    }
    /// A download or an install is running: the game must not start meanwhile.
    pub fn working(&self) -> bool {
        matches!(self.state(), Update::Downloading { .. } | Update::Installing)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    fn set(&self, s: Update) {
        *self.state.lock().unwrap() = s;
    }
}

pub fn pref(work: &Path, key: &str, default: bool) -> bool {
    read_prefs(work).get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn read_prefs(work: &Path) -> Map<String, Value> {
    fs::read_to_string(work.join(PREFS_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Other keys in the file are kept as they are.
pub fn set_pref(work: &Path, key: &str, on: bool) -> Result<(), String> {
    let mut prefs = read_prefs(work);
    prefs.insert(key.into(), Value::Bool(on));
    let tmp = work.join(format!("{PREFS_FILE}.tmp"));
    let text = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
    fs::write(&tmp, text + "\n").and_then(|_| fs::rename(&tmp, work.join(PREFS_FILE))).map_err(|e| e.to_string())
}

fn agent(per_read: Duration) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(format!("ForgottenLandLauncher/{CURRENT}"))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_per_read(Some(per_read))
        .build()
        .into()
}

fn fetch_releases() -> Result<Vec<GhRelease>, String> {
    let base = std::env::var(API_ENV).unwrap_or_else(|_| DEFAULT_API.into());
    let mut resp = agent(Duration::from_secs(15))
        .get(format!("{}/repos/{REPO}/releases?per_page=30", base.trim_end_matches('/')))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&resp.body_mut().read_to_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Asks GitHub for the newest release. `manual` is the About button: only then is "no internet" worth saying.
pub fn check(u: &Updater, work: &Path, manual: bool) {
    {
        let mut s = u.state.lock().unwrap();
        if matches!(*s, Update::Checking | Update::Downloading { .. } | Update::Installing) {
            return;
        }
        *s = Update::Checking;
    }
    let current = Version::parse(CURRENT).expect("the launcher's own version");
    let found = fetch_releases().map(|all| pick_release(&all, &current, pref(work, STABLE_KEY, true)));
    u.set(match found {
        Ok(Some(release)) => Update::Available(release),
        Ok(None) => Update::UpToDate,
        Err(e) if manual => Update::Failed { msg: format!("Could not check for updates: {e}") },
        Err(_) => Update::Idle,
    });
}

/// RecompLauncher keeps its library (every game it installed, with its folder) here, under its own data folder.
const RECOMPLAUNCHER_DATA: &str = "com.recomplauncher.app";

/// A folder the way RecompLauncher compares them (its `same_path`): slash direction, a trailing slash and letter case do not matter.
fn norm(p: &Path) -> String {
    let s = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().replace('\\', "/");
    s.trim_start_matches("//?/").trim_end_matches('/').to_lowercase()
}

/// Whether RecompLauncher's library (library.json) lists `work` as the folder of a game it installed, or a folder inside one.
/// A build the player added from a folder (`local`) is never updated by RecompLauncher, so it does not count.
pub fn managed_by_recomplauncher(library_json: &str, work: &Path) -> bool {
    let Ok(lib) = serde_json::from_str::<Value>(library_json) else { return false };
    let Some(games) = lib.get("installed").and_then(Value::as_object) else { return false };
    let work = norm(work);
    games.values().filter(|g| !g.get("local").and_then(Value::as_bool).unwrap_or(false)).filter_map(|g| g.get("path")?.as_str()).any(|path| {
        let path = norm(Path::new(path));
        !path.is_empty() && (work == path || work.starts_with(&format!("{path}/")))
    })
}

/// True when RecompLauncher on this PC installed this copy, so RecompLauncher is what updates it.
pub fn managed(work: &Path) -> bool {
    let Some(appdata) = std::env::var_os("APPDATA") else { return false };
    fs::read_to_string(Path::new(&appdata).join(RECOMPLAUNCHER_DATA).join("library.json")).is_ok_and(|t| managed_by_recomplauncher(&t, work))
}

/// The folder the last update used: the old files wait there because the running exe cannot be deleted.
pub fn cleanup(work: &Path) {
    let _ = fs::remove_dir_all(work.join(STAGING));
}

fn is_protected(rel: &Path) -> bool {
    let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
    let name = rel.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    PROTECTED.contains(&first.as_str()) || name.ends_with(".log")
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(format!("{:x}", h.finalize()));
        }
        h.update(&buf[..n]);
    }
}

fn download(u: &Updater, release: &Release, dest: &Path) -> Result<(), String> {
    let resp = agent(Duration::from_secs(30)).get(&release.asset).call().map_err(|e| format!("The download failed: {e}"))?;
    let total = resp.body().content_length().filter(|n| *n > 0);
    let mut body = resp.into_body().into_reader();
    let mut out = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let (mut buf, mut got) = (vec![0u8; 1 << 16], 0u64);
    loop {
        if u.cancel.load(Ordering::Relaxed) {
            return Err("Cancelled.".into());
        }
        let n = body.read(&mut buf).map_err(|e| format!("The download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if let Some(total) = total {
            u.set(Update::Downloading { progress: (got as f32 / total as f32).min(1.0) });
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    if format!("{:x}", hash.finalize()) != release.digest {
        return Err("The download does not match the checksum GitHub lists for it, so it was not installed. Try again later.".into());
    }
    Ok(())
}

/// The zip's files without its top folder, written under `dest`. A file the update must not touch is left out.
fn unpack(zip: &Path, dest: &Path) -> Result<Vec<PathBuf>, String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else { return Err("The update holds a file with an unsafe path.".into()) };
        let rel: PathBuf = name.components().skip(1).collect();
        if !entry.is_file() || rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) || is_protected(&rel) {
            continue;
        }
        let to = dest.join(&rel);
        fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut fs::File::create(&to).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        files.push(rel);
    }
    Ok(files)
}

/// Each file in `files` goes from `new` into `work`, the one it replaces moved to `old` first (a running exe can be
/// renamed, not overwritten). A file that is the same already stays untouched, so the runtime keeps its time stamp and
/// the shaders and game code are only prepared again when the runtime really changed. Any failure puts everything back.
fn swap(work: &Path, new: &Path, old: &Path, files: &[PathBuf]) -> Result<(), String> {
    let mut done: Vec<(PathBuf, bool)> = Vec::new();
    let result = files.iter().try_for_each(|rel| -> std::io::Result<()> {
        let (to, from, aside) = (work.join(rel), new.join(rel), old.join(rel));
        let had = to.exists();
        if had && fs::metadata(&to)?.len() == fs::metadata(&from)?.len() && sha256_file(&to)? == sha256_file(&from)? {
            return Ok(());
        }
        fs::create_dir_all(to.parent().unwrap())?;
        if had {
            fs::create_dir_all(aside.parent().unwrap())?;
            fs::rename(&to, &aside)?;
        }
        done.push((rel.clone(), had));
        fs::rename(&from, &to)
    });
    if let Err(e) = result {
        for (rel, had) in done.iter().rev() {
            let _ = fs::remove_file(work.join(rel));
            if *had {
                let _ = fs::rename(old.join(rel), work.join(rel));
            }
        }
        return Err(format!("The update could not be installed ({e}). Your files were put back. Close anything using this folder, such as an antivirus scan, and try again."));
    }
    Ok(())
}

/// Download, check, unpack and swap in. Ok means the files are in place and the launcher should restart.
pub fn apply(u: &Updater, work: &Path, release: &Release) -> Result<(), String> {
    u.cancel.store(false, Ordering::Relaxed);
    u.set(Update::Downloading { progress: 0.0 });
    let stage = work.join(STAGING);
    let result = (|| {
        let _ = fs::remove_dir_all(&stage);
        fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
        let zip = stage.join("download.zip");
        download(u, release, &zip)?;
        u.set(Update::Installing);
        let files = unpack(&zip, &stage.join("new"))?;
        fs::remove_file(&zip).map_err(|e| e.to_string())?;
        swap(work, &stage.join("new"), &stage.join("old"), &files)
    })();
    match &result {
        Ok(()) => {}
        Err(e) if e == "Cancelled." => {
            cleanup(work);
            u.set(Update::Available(release.clone()));
        }
        Err(e) => {
            cleanup(work);
            u.set(Update::Failed { msg: e.clone() });
        }
    }
    result
}

pub fn fail(u: &Updater, msg: String) {
    u.set(Update::Failed { msg });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("kfl-update-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    const LIBRARY: &str = r#"{"format":2,"installed":{
        "kirby-forgotten-land-recompiled":{"id":"kirby-forgotten-land-recompiled","repo":"scarxity/kirby-forgotten-land-recompiled","tag":"v0.3.2","path":"C:\\Recomp Library\\Kirby"},
        "mine":{"id":"mine","local":true,"tag":"own","path":"D:\\dev\\kfl"},
        "other":{"id":"other","tag":"v1","path":"C:\\Recomp Library\\Other"}},
        "settings":{"library_dir":"C:\\Recomp Library"}}"#;

    #[test]
    fn a_copy_in_a_recomplauncher_game_folder_is_managed() {
        for work in [r"C:\Recomp Library\Kirby", r"c:/recomp library/kirby/", r"C:\Recomp Library\Kirby\ForgottenLandRecomp-0.3.2"] {
            assert!(managed_by_recomplauncher(LIBRARY, Path::new(work)), "{work}");
        }
    }

    #[test]
    fn other_folders_are_not_managed() {
        for work in [r"C:\Recomp Library", r"C:\Recomp Library\Kirby2", r"E:\Games\Kirby", r"D:\dev\kfl"] {
            assert!(!managed_by_recomplauncher(LIBRARY, Path::new(work)), "{work}");
        }
    }

    #[test]
    fn an_unreadable_or_empty_library_manages_nothing() {
        let work = Path::new(r"C:\Recomp Library\Kirby");
        for text in ["", "{broken", "{}", r#"{"installed":[]}"#, r#"{"installed":{"x":{"path":""}}}"#, r#"{"installed":{"x":{"tag":"v1"}}}"#] {
            assert!(!managed_by_recomplauncher(text, work), "{text}");
        }
    }

    #[test]
    fn versions_follow_semver_with_betas_below_their_stable() {
        assert!(v("0.3.3-beta.1") < v("0.3.3-beta.2"));
        assert!(v("0.3.3-beta.2") < v("0.3.3"));
        assert!(v("0.3.2") < v("0.3.3-beta.1"));
        assert!(v("0.3.10") > v("0.3.9"));
        assert_eq!(v("v1.0.0"), v("1.0.0"));
        assert_eq!(v("0.3.3-beta.2").to_string(), "0.3.3-beta.2");
        assert_eq!(v("0.3.3").to_string(), "0.3.3");
        for bad in ["", "0.3", "0.3.3.1", "0.3.x", "0.3.3-rc.1", "0.3.3-beta", "0.3.3-beta.x", "latest"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    fn release(tag: &str, pre: bool, digest: Option<&str>) -> String {
        let asset = format!(
            r#"{{"name":"ForgottenLandRecomp-{tag}-Windows-x64.zip","browser_download_url":"http://x/{tag}.zip","digest":{}}}"#,
            digest.map_or("null".into(), |d| format!("\"sha256:{d}\""))
        );
        format!(r#"{{"tag_name":"{tag}","body":"notes {tag}","draft":false,"prerelease":{pre},"assets":[{asset}]}}"#)
    }

    fn releases(list: &[String]) -> Vec<GhRelease> {
        serde_json::from_str(&format!("[{}]", list.join(","))).unwrap()
    }

    #[test]
    fn the_newest_release_on_the_channel_is_picked() {
        let all = releases(&[release("0.3.4-beta.1", true, Some("AB")), release("0.3.3", false, Some("cd")), release("0.3.2", false, Some("ef"))]);
        let stable = pick_release(&all, &v("0.3.2"), true).unwrap();
        assert_eq!((stable.version.as_str(), stable.notes.as_str(), stable.digest.as_str()), ("0.3.3", "notes 0.3.3", "cd"));
        let beta = pick_release(&all, &v("0.3.2"), false).unwrap();
        assert_eq!((beta.version.as_str(), beta.digest.as_str()), ("0.3.4-beta.1", "ab"), "the digest is compared in lower case");
        assert_eq!(pick_release(&all, &v("0.3.3"), true), None, "already current");
    }

    #[test]
    fn a_beta_player_gets_the_next_beta_then_the_stable_but_never_a_downgrade() {
        let betas = releases(&[release("0.3.3-beta.2", true, Some("b2")), release("0.3.3-beta.1", true, Some("b1")), release("0.3.2", false, Some("aa"))]);
        assert_eq!(pick_release(&betas, &v("0.3.3-beta.1"), false).unwrap().version, "0.3.3-beta.2");
        assert_eq!(pick_release(&betas, &v("0.3.3-beta.2"), true), None, "stable 0.3.2 is older than the beta in use");
        assert_eq!(pick_release(&betas, &v("0.3.3-beta.2"), false), None);
        let promoted = releases(&[release("0.3.3", false, Some("cc")), release("0.3.3-beta.2", true, Some("b2"))]);
        assert_eq!(pick_release(&promoted, &v("0.3.3-beta.2"), true).unwrap().version, "0.3.3");
    }

    #[test]
    fn a_stable_channel_never_offers_a_beta_even_when_it_is_not_marked_pre_release() {
        let all = releases(&[release("0.3.4-beta.1", false, Some("b1"))]);
        assert_eq!(pick_release(&all, &v("0.3.2"), true), None);
    }

    #[test]
    fn releases_that_cannot_be_installed_or_verified_are_skipped() {
        let mut all = releases(&[release("0.3.4", false, None), release("0.3.3", false, Some("cc")), release("weekly", false, Some("dd"))]);
        assert_eq!(pick_release(&all, &v("0.3.2"), true).unwrap().version, "0.3.3", "0.3.4 has no digest");
        all[1].draft = true;
        assert_eq!(pick_release(&all, &v("0.3.2"), true), None, "drafts are not public");
        let mut other = releases(&[release("0.3.3", false, Some("cc"))]);
        other[0].assets[0].name = "ForgottenLandRecomp-0.3.3-Source.zip".into();
        assert_eq!(pick_release(&other, &v("0.3.2"), true), None, "no Windows zip");
    }

    #[test]
    fn the_state_goes_to_the_page_as_one_tagged_object() {
        let json = |u: &Update| serde_json::to_string(u).unwrap();
        assert_eq!(json(&Update::Idle), r#"{"state":"idle"}"#);
        assert_eq!(json(&Update::Downloading { progress: 0.5 }), r#"{"state":"downloading","progress":0.5}"#);
        let r = pick_release(&releases(&[release("0.3.3", false, Some("cc"))]), &v("0.3.2"), true).unwrap();
        assert_eq!(json(&Update::Available(r)), r#"{"state":"available","version":"0.3.3","notes":"notes 0.3.3","asset":"http://x/0.3.3.zip","digest":"cc"}"#);
    }

    #[test]
    fn preferences_default_and_keep_keys_that_are_not_ours() {
        let work = scratch("prefs");
        assert!(!pref(&work, AUTO_KEY, false) && pref(&work, STABLE_KEY, true));
        fs::write(work.join(PREFS_FILE), r#"{"minimise": true}"#).unwrap();
        set_pref(&work, AUTO_KEY, true).unwrap();
        set_pref(&work, STABLE_KEY, false).unwrap();
        assert!(pref(&work, AUTO_KEY, false) && !pref(&work, STABLE_KEY, true));
        assert!(pref(&work, "minimise", false), "another page's key survives");
        fs::write(work.join(PREFS_FILE), "not json").unwrap();
        assert!(!pref(&work, AUTO_KEY, false), "a damaged file reads as the defaults");
        fs::remove_dir_all(&work).unwrap();
    }

    fn make_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, text) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
            w.write_all(text.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    /// Serves `body` for every request until the test ends.
    fn serve(body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/asset.zip", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let mut line = String::new();
                let mut r = BufReader::new(s.try_clone().unwrap());
                while r.read_line(&mut line).is_ok_and(|n| n > 2) {
                    line.clear();
                }
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
                let _ = s.write_all(&body);
            }
        });
        url
    }

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn install_with_files(name: &str) -> PathBuf {
        let work = scratch(name);
        for (name, text) in [("ForgottenLandLauncher.exe", "old launcher"), ("same.dll", "same"), ("larecomp.ini", "fullscreen = 1"), ("game-folder.txt", "D:\\game"), ("extracted/lasgame.dll", "built"), ("save/slot1", "save"), ("game.log", "log"), ("mods/m/romfs/a", "mod")] {
            fs::create_dir_all(work.join(name).parent().unwrap()).unwrap();
            fs::write(work.join(name), text).unwrap();
        }
        work
    }

    fn tree(work: &Path) -> Vec<(String, String)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
            for e in fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, base, out);
                } else {
                    out.push((p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), fs::read_to_string(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(work, work, &mut out);
        out.sort();
        out
    }

    fn fake(version: &str, url: String, digest: String) -> Release {
        Release { version: version.into(), notes: String::new(), asset: url, digest }
    }

    #[test]
    fn an_update_replaces_the_programs_files_and_nothing_of_the_players() {
        let work = install_with_files("replace");
        let zip = make_zip(&[
            ("ForgottenLandRecomp/ForgottenLandLauncher.exe", "new launcher"),
            ("ForgottenLandRecomp/same.dll", "same"),
            ("ForgottenLandRecomp/zig/zig.exe", "zig"),
            ("ForgottenLandRecomp/licenses/a.txt", "licence"),
            ("ForgottenLandRecomp/larecomp.ini", "evil"),
            ("ForgottenLandRecomp/game-folder.txt", "evil"),
            ("ForgottenLandRecomp/save/slot1", "evil"),
            ("ForgottenLandRecomp/extracted/lasgame.dll", "evil"),
            ("ForgottenLandRecomp/mods/m/romfs/a", "evil"),
            ("ForgottenLandRecomp/game.log", "evil"),
            ("ForgottenLandRecomp/Cache/shader", "evil"),
        ]);
        let same_time = fs::metadata(work.join("same.dll")).unwrap().modified().unwrap();
        let u = Updater::default();
        apply(&u, &work, &fake("0.3.3", serve(zip.clone()), digest(&zip))).unwrap();
        assert_eq!(
            tree(&work),
            [
                (".update/new/same.dll", "same"),
                (".update/old/ForgottenLandLauncher.exe", "old launcher"),
                ("ForgottenLandLauncher.exe", "new launcher"),
                ("extracted/lasgame.dll", "built"),
                ("game-folder.txt", "D:\\game"),
                ("game.log", "log"),
                ("larecomp.ini", "fullscreen = 1"),
                ("licenses/a.txt", "licence"),
                ("mods/m/romfs/a", "mod"),
                ("same.dll", "same"),
                ("save/slot1", "save"),
                ("zig/zig.exe", "zig"),
            ]
            .map(|(a, b)| (a.to_string(), b.to_string()))
        );
        assert_eq!(fs::metadata(work.join("same.dll")).unwrap().modified().unwrap(), same_time, "an unchanged file is not rewritten");
        cleanup(&work);
        assert!(!work.join(STAGING).exists());
        fs::remove_dir_all(&work).unwrap();
    }

    #[test]
    fn a_bad_digest_installs_nothing() {
        let work = install_with_files("digest");
        let before = tree(&work);
        let zip = make_zip(&[("ForgottenLandRecomp/ForgottenLandLauncher.exe", "new launcher")]);
        let u = Updater::default();
        let err = apply(&u, &work, &fake("0.3.3", serve(zip), digest(b"something else"))).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert_eq!(tree(&work), before);
        assert!(matches!(u.state(), Update::Failed { .. }));
        fs::remove_dir_all(&work).unwrap();
    }

    #[test]
    fn a_failure_part_way_puts_the_old_files_back() {
        let work = install_with_files("rollback");
        fs::write(work.join("a.dll"), "old a").unwrap();
        let before = tree(&work);
        let stage = work.join(STAGING);
        fs::create_dir_all(stage.join("new")).unwrap();
        fs::write(stage.join("new").join("a.dll"), "new a").unwrap();
        // the second file is not in the staging folder: the move fails after a.dll was already replaced
        let files = vec![PathBuf::from("a.dll"), PathBuf::from("missing.dll")];
        let err = swap(&work, &stage.join("new"), &stage.join("old"), &files).unwrap_err();
        assert!(err.contains("put back"), "{err}");
        fs::remove_dir_all(&stage).unwrap();
        assert_eq!(tree(&work), before);
        fs::remove_dir_all(&work).unwrap();
    }

    #[test]
    fn cancel_stops_a_download() {
        let work = scratch("cancel");
        let zip = make_zip(&[("ForgottenLandRecomp/x.dll", "x")]);
        let u = Updater::default();
        u.cancel();
        assert_eq!(download(&u, &fake("0.3.3", serve(zip.clone()), digest(&zip)), &work.join("d.zip")), Err("Cancelled.".into()));
        fs::remove_dir_all(&work).unwrap();
    }

    #[test]
    fn the_players_data_is_protected_by_name() {
        for p in ["larecomp.ini", "LARECOMP.INI", "save/a", "extracted/x/y", "cache/z", "mods/m", "game-folder.txt", "launcher-prefs.json", "crash-20260101-000000.log", "zig/zig.log", ".update/x"] {
            assert!(is_protected(Path::new(p)), "{p}");
        }
        for p in ["ForgottenLandLauncher.exe", "zig/zig.exe", "licenses/a.txt", "README.txt", "recomplauncher.json", "gamelib-src/cpu.h"] {
            assert!(!is_protected(Path::new(p)), "{p}");
        }
    }
}
