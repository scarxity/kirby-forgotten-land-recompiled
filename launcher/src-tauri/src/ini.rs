//! larecomp.ini (the game's settings) and launcher-prefs.json (the launcher's own, flat keys), edited one entry at a time:
//! whatever else is in the file, key bindings and comments included, is kept as it is.
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

/// Every `key = value` line, in file order. The game takes the last entry of a key.
pub fn read(path: &Path) -> Vec<(String, String)> {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().into(), v.trim().into())).collect()
}

/// `text` with each entry's line replaced where it is, or added at the end.
pub fn apply(text: &str, entries: &[(String, String)]) -> String {
    let mut done = vec![false; entries.len()];
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| {
            let key = l.split_once('=').map(|(k, _)| k.trim());
            match entries.iter().position(|(k, _)| Some(k.as_str()) == key) {
                Some(i) => {
                    done[i] = true;
                    format!("{} = {}", entries[i].0, entries[i].1)
                }
                None => l.to_string(),
            }
        })
        .collect();
    for (i, (k, v)) in entries.iter().enumerate() {
        if !done[i] {
            lines.push(format!("{k} = {v}"));
        }
    }
    lines.join("\n") + "\n"
}

fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub fn write(path: &Path, entries: &[(String, String)]) -> Result<(), String> {
    if entries.iter().any(|(k, v)| !valid_key(k) || v.contains(['\n', '\r'])) {
        return Err("Unknown setting.".into());
    }
    let text = fs::read_to_string(path).unwrap_or_default();
    fs::write(path, apply(&text, entries)).map_err(|e| e.to_string())
}

pub fn read_prefs(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Written beside the file and renamed over it, so the other reader never sees half of it.
pub fn write_prefs(path: &Path, values: Map<String, Value>) -> Result<(), String> {
    if values.keys().any(|k| !valid_key(k)) || values.values().any(|v| v.is_object() || v.is_array()) {
        return Err("Unknown preference.".into());
    }
    let mut all = read_prefs(path);
    all.extend(values);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    fs::write(&tmp, text + "\n").and_then(|_| fs::rename(&tmp, path)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(k: &str, v: &str) -> (String, String) {
        (k.into(), v.into())
    }

    #[test]
    fn a_change_replaces_its_line_and_keeps_every_other_line() {
        let ini = "# settings\r\nbind_key_jump = 44\r\nvolume = 0.50\r\nbind_pad_a = 3\r\nfullscreen = 0\r\n";
        let out = apply(ini, &[e("volume", "1.00"), e("fullscreen", "1"), e("antialias", "smaa")]);
        assert_eq!(out, "# settings\nbind_key_jump = 44\nvolume = 1.00\nbind_pad_a = 3\nfullscreen = 1\nantialias = smaa\n");
    }

    #[test]
    fn a_key_written_twice_ends_with_one_value() {
        assert_eq!(apply("volume = 1\nvolume = 2\n", &[e("volume", "0.30")]), "volume = 0.30\nvolume = 0.30\n");
    }

    #[test]
    fn a_line_for_a_setting_the_launcher_no_longer_shows_is_kept() {
        let ini = "aspect = unlocked
volume = 0.50
";
        assert_eq!(apply(ini, &[e("volume", "1.00")]), "aspect = unlocked
volume = 1.00
");
    }

    #[test]
    fn a_missing_file_gets_just_the_entries() {
        assert_eq!(apply("", &[e("fullscreen", "1")]), "fullscreen = 1\n");
    }

    #[test]
    fn a_comment_mentioning_a_key_is_not_that_key() {
        assert_eq!(apply("# volume = 9\n", &[e("volume", "1.00")]), "# volume = 9\nvolume = 1.00\n");
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("kfl-ini-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn written_settings_read_back_and_unknown_keys_survive() {
        let dir = scratch("ini");
        let path = dir.join("larecomp.ini");
        fs::write(&path, "bind_key_jump = 44\nresolution = auto\n").unwrap();
        write(&path, &[e("resolution", "75%"), e("language", "zh-Hans")]).unwrap();
        assert_eq!(read(&path), vec![e("bind_key_jump", "44"), e("resolution", "75%"), e("language", "zh-Hans")]);
        assert!(write(&path, &[e("a b", "1")]).is_err());
        assert!(write(&path, &[e("volume", "1\nfullscreen = 1")]).is_err());
        assert_eq!(read(&path).len(), 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prefs_are_merged_and_nothing_else_is_lost() {
        let dir = scratch("prefs");
        let path = dir.join("launcher-prefs.json");
        assert!(read_prefs(&path).is_empty());
        fs::write(&path, r#"{"check_updates": true}"#).unwrap();
        let mut set = Map::new();
        set.insert("minimise_on_play".into(), Value::Bool(false));
        write_prefs(&path, set).unwrap();
        let got = read_prefs(&path);
        assert_eq!(got["check_updates"], Value::Bool(true));
        assert_eq!(got["minimise_on_play"], Value::Bool(false));
        let mut bad = Map::new();
        bad.insert("nested".into(), serde_json::json!({"a": 1}));
        assert!(write_prefs(&path, bad).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
