//! Local plugin packages. No downloads, installer hooks or automatic adoption.
//! Providers are trusted executable code, but are never executed by this module.
use crate::{applets, config::Config, files};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const META: &str = ".plugins";
const MAX_TREE: usize = 512;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const SECTIONS: [&str; 4] = ["left", "center", "right", "drawer"];
const BUNDLED_APPLETS: [&str; 6] = [
    "weather",
    "wifi",
    "calendar",
    "timezones",
    "volume",
    "_template",
];
const BUNDLED_THEMES: [&str; 4] = [
    "catppuccin-mocha",
    "catppuccin-latte",
    "dynamic-dark",
    "dynamic-light",
];
pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Applet,
    Theme,
}
impl Kind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "applet" => Ok(Self::Applet),
            "theme" => Ok(Self::Theme),
            _ => Err("kind must be applet or theme".into()),
        }
    }
    fn directory(self) -> &'static str {
        match self {
            Self::Applet => "applets",
            Self::Theme => "themes",
        }
    }
    fn key(self, id: &str) -> String {
        format!("{}-{id}", self.directory())
    }
    fn bundled(self, id: &str) -> bool {
        match self {
            Self::Applet => BUNDLED_APPLETS.contains(&id),
            Self::Theme => BUNDLED_THEMES.contains(&id),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub schema: u32,
    pub id: String,
    pub kind: Kind,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}
impl Package {
    fn validate(&self) -> Result<()> {
        valid_id(&self.id)?;
        if self.schema != 1
            || self.name.is_empty()
            || self.name.len() > 128
            || self.description.len() > 1024
            || self.version.is_empty()
            || self.version.len() > 64
            || !self
                .version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c))
        {
            return Err("invalid package metadata (schema must be 1)".into());
        }
        if self.kind.bundled(&self.id) || crate::config::BUILTIN_MODULES.contains(&self.id.as_str())
        {
            return Err("reserved built-in identifier".into());
        }
        Ok(())
    }
}
fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 32
        || !id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_".contains(&c))
    {
        return Err("id must contain 1..32 lowercase ASCII letters, digits, - or _".into());
    }
    if windows_reserved(id) {
        return Err("reserved Windows filename".into());
    }
    Ok(())
}
fn windows_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    package: Package,
    source: String,
    files: BTreeMap<String, String>,
    // Upstream baseline (not merged local settings) for three-way updates.
    applet_manifest: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Installed {
    pub id: String,
    pub kind: Kind,
    pub origin: String,
    pub managed: bool,
    pub version: Option<String>,
    pub source: Option<String>,
    pub status: String,
    pub error: Option<String>,
}
fn metadata(path: &Path) -> Result<fs::Metadata> {
    let m = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(windows)]
    let link = {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let link = m.file_type().is_symlink();
    if link || (!m.is_file() && !m.is_dir()) {
        return Err(format!(
            "{}: links/reparse/special files are refused",
            path.display()
        ));
    }
    Ok(m)
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            metadata(path)?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
fn directory(path: &Path) -> Result<()> {
    if !metadata(path)?.is_dir() {
        return Err(format!("{}: expected directory", path.display()));
    }
    Ok(())
}
fn mkdir(path: &Path) -> Result<()> {
    if !exists(path)? {
        fs::create_dir(path).map_err(|e| e.to_string())?;
    }
    directory(path)
}
fn text(path: &Path) -> Result<String> {
    String::from_utf8(files::read_config(path)?).map_err(|e| e.to_string())
}
fn json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&files::read_bounded(path, 256 * 1024)?)
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn record_path(home: &Path, kind: Kind, id: &str) -> PathBuf {
    home.join(META)
        .join("records")
        .join(format!("{}.json", kind.key(id)))
}
fn receipt(home: &Path, kind: Kind, id: &str) -> Result<Option<Receipt>> {
    let meta = home.join(META);
    if !exists(&meta)? {
        return Ok(None);
    }
    directory(&meta)?;
    let records = meta.join("records");
    if !exists(&records)? {
        return Ok(None);
    }
    directory(&records)?;
    let path = record_path(home, kind, id);
    if !exists(&path)? {
        return Ok(None);
    }
    let r: Receipt = json(&path)?;
    r.package.validate()?;
    if r.package.id != id || r.package.kind != kind {
        return Err("receipt identity mismatch".into());
    }
    Ok(Some(r))
}
fn paths(kind: Kind, id: &str) -> Vec<PathBuf> {
    match kind {
        Kind::Applet => vec![PathBuf::from(format!("applets/{id}"))],
        Kind::Theme => vec![
            PathBuf::from(format!("themes/{id}")),
            PathBuf::from(format!("themes/{id}.toml")),
        ],
    }
}
/// Bounded tree walk, with Windows case-collision checks even when preparing on Linux.
fn tree(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut out = Vec::new();
    let mut count = 0;
    let mut total = 0;
    while let Some((path, depth)) = pending.pop() {
        count += 1;
        if count > MAX_TREE || depth > 16 {
            return Err("package tree exceeds 512 entries / 16 levels".into());
        }
        let m = metadata(&path)?;
        if m.is_dir() {
            let mut names = std::collections::BTreeSet::new();
            for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "non-UTF8 filename")?;
                if !illium_theme::pack::plain_name(&name)
                    || windows_reserved(&name)
                    || !names.insert(name.to_lowercase())
                {
                    return Err(format!("invalid or case-colliding filename: {name}"));
                }
                if pending.len() + count >= MAX_TREE {
                    return Err("package tree exceeds 512 entries".into());
                }
                pending.push((entry.path(), depth + 1));
            }
        } else {
            total += m.len();
            if total > MAX_BYTES {
                return Err("package exceeds 512 MiB".into());
            }
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}
fn copy(source: &Path, target: &Path) -> Result<()> {
    if metadata(source)?.is_file() {
        fs::copy(source, target).map_err(|e| e.to_string())?;
    } else {
        fs::create_dir(target).map_err(|e| e.to_string())?;
        for path in tree(source)? {
            let dest = target.join(path.strip_prefix(source).map_err(|e| e.to_string())?);
            fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::copy(path, dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn digest(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut bytes = 0u64;
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > MAX_BYTES {
            return Err("file exceeds package limit".into());
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn fingerprints(home: &Path, kind: Kind, id: &str) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for relative in paths(kind, id) {
        let path = home.join(relative);
        if !exists(&path)? {
            continue;
        }
        for file in tree(&path)? {
            result.insert(
                file.strip_prefix(home)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                digest(&file)?,
            );
        }
    }
    Ok(result)
}
fn bar(home: &Path) -> Result<toml_edit::DocumentMut> {
    text(&home.join("bar.toml"))?
        .parse()
        .map_err(|e| format!("bar.toml: {e}"))
}
fn modules(doc: &toml_edit::DocumentMut) -> Vec<String> {
    SECTIONS
        .iter()
        .flat_map(|s| doc.get(s).and_then(|v| v.as_array()).into_iter().flatten())
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}
fn active_theme(home: &Path) -> Result<String> {
    let global: toml::Value =
        toml::from_str(&text(&home.join("illium.toml"))?).map_err(|e| e.to_string())?;
    global
        .get("theme")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or("missing active theme".into())
}
/// Discovery does not write receipts, infer versions, or require a catalogue.
pub fn list(home: &Path) -> Result<Vec<Installed>> {
    directory(home)?;
    ensure_ready(home)?;
    let disabled = applets::disabled(home)?;
    let modules = modules(&bar(home)?);
    let active = active_theme(home)?;
    let mut result = Vec::new();
    for kind in [Kind::Applet, Kind::Theme] {
        let parent = home.join(kind.directory());
        if !exists(&parent)? {
            continue;
        }
        directory(&parent)?;
        let entries = fs::read_dir(&parent).map_err(|e| e.to_string())?;
        for (index, entry) in entries.enumerate() {
            if index >= 256 {
                return Err("plugin directory exceeds 256 entries".into());
            }
            let path = entry.map_err(|e| e.to_string())?.path();
            let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            let id = match kind {
                Kind::Applet
                    if applets::valid_name(name)
                        && name != "_template"
                        && path.join("applet.toml").is_file() =>
                {
                    name.to_owned()
                }
                Kind::Theme if path.extension().is_some_and(|v| v == "toml") => {
                    path.file_stem().unwrap().to_string_lossy().into_owned()
                }
                _ => continue,
            };
            let validation = metadata(&path).and_then(|_| match kind {
                Kind::Applet => applets::load(home, &id).map(|_| ()),
                Kind::Theme => illium_theme::Theme::load(home, &id).map(|_| ()),
            });
            let r = receipt(home, kind, &id);
            let error = validation.err().or_else(|| r.as_ref().err().cloned());
            let r = r.ok().flatten();
            let enabled = match kind {
                Kind::Theme => id == active,
                Kind::Applet => {
                    !disabled.contains(&id)
                        && (modules.contains(&id)
                            || applets::load(home, &id)
                                .ok()
                                .and_then(|a| a.manifest.attach)
                                .is_some_and(|m| {
                                    modules.contains(&m)
                                        || (m == "time" && modules.iter().any(|m| m == "clock"))
                                }))
                }
            };
            result.push(Installed {
                id: id.clone(),
                kind,
                origin: if kind.bundled(&id) {
                    "bundled"
                } else if r.is_some() {
                    "local-package"
                } else {
                    "unmanaged"
                }
                .into(),
                managed: r.is_some(),
                version: r.as_ref().map(|r| r.package.version.clone()),
                source: r.as_ref().map(|r| r.source.clone()),
                status: if error.is_some() {
                    "error"
                } else if enabled {
                    if kind == Kind::Theme {
                        "active"
                    } else {
                        "enabled"
                    }
                } else if kind == Kind::Theme {
                    "installed"
                } else {
                    "disabled"
                }
                .into(),
                error,
            });
        }
    }
    result.sort_by(|a, b| (a.kind.directory(), &a.id).cmp(&(b.kind.directory(), &b.id)));
    Ok(result)
}

/// Fail closed during publication or after an interrupted transaction. The
/// watcher keeps the last valid configuration until the manager removes this gate.
pub fn ensure_ready(home: &Path) -> Result<()> {
    let meta = home.join(META);
    if exists(&meta)? {
        directory(&meta)?;
        if exists(&meta.join("pending.json"))? {
            return Err(format!(
                "unfinished plugin transaction: {}; inspect its backup journal before recovery",
                meta.join("pending.json").display()
            ));
        }
    }
    Ok(())
}

/// A manager lock prevents concurrent manager writes. A crash leaves a pending
/// backup for manual recovery; further mutations fail closed, never guess.
type Signature = (bool, BTreeMap<PathBuf, String>);
struct Session {
    home: PathBuf,
    _lock: fs::File,
    observed: BTreeMap<PathBuf, Option<Signature>>,
}
impl Session {
    fn open(home: &Path) -> Result<Self> {
        directory(home)?;
        let home = fs::canonicalize(home).map_err(|e| e.to_string())?;
        mkdir(&home.join(META))?;
        mkdir(&home.join(META).join("records"))?;
        mkdir(&home.join(META).join("backups"))?;
        let lock_path = home.join(META).join("lock");
        if exists(&lock_path)? && !metadata(&lock_path)?.is_file() {
            return Err("invalid manager lock".into());
        }
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|e| format!("another plugin operation is running: {e}"))?;
        ensure_ready(&home)?;
        let mut session = Self {
            home,
            _lock: lock,
            observed: BTreeMap::new(),
        };
        session.observe(&[
            "bar.toml".into(),
            "plugins.toml".into(),
            "illium.toml".into(),
        ])?;
        Ok(session)
    }
    fn observe(&mut self, paths: &[PathBuf]) -> Result<()> {
        for relative in paths {
            let path = self.home.join(relative);
            self.observed
                .entry(relative.clone())
                .or_insert(if exists(&path)? {
                    Some(signature(&path)?)
                } else {
                    None
                });
        }
        Ok(())
    }
    fn check_observed(&self) -> Result<()> {
        for (relative, expected) in &self.observed {
            let path = self.home.join(relative);
            let actual = if exists(&path)? {
                Some(signature(&path)?)
            } else {
                None
            };
            if &actual != expected {
                return Err(format!(
                    "{} changed while preparing the operation",
                    relative.display()
                ));
            }
        }
        Ok(())
    }
    /// Each replacement is staged on the config volume. Backups survive success
    /// and failure. Publication errors roll back in reverse order.
    fn commit(
        &self,
        changes: Vec<(PathBuf, Option<PathBuf>)>,
        validate: impl FnOnce() -> Result<()>,
    ) -> Result<PathBuf> {
        self.check_observed()?;
        let backup = tempfile::Builder::new()
            .prefix("transaction-")
            .tempdir_in(self.home.join(META).join("backups"))
            .map_err(|e| e.to_string())?
            .keep();
        let mut before = Vec::new();
        let mut expected_after = Vec::new();
        for (i, (relative, replacement)) in changes.iter().enumerate() {
            let target = self.home.join(relative);
            directory(target.parent().unwrap())?;
            let present = exists(&target)?;
            if present {
                copy(&target, &backup.join(format!("before-{i}")))?;
            }
            before.push(present);
            expected_after.push(match replacement {
                Some(path) if exists(path)? => Some(signature(path)?),
                _ => None,
            });
        }
        self.check_observed()?;
        let journal = serde_json::json!({"backup": backup, "paths": changes.iter().map(|(p,_)| p.to_string_lossy()).collect::<Vec<_>>(), "existed": before});
        let bytes = serde_json::to_vec_pretty(&journal).map_err(|e| e.to_string())?;
        let mut file = fs::File::create(backup.join("pending.json")).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::hard_link(
            backup.join("pending.json"),
            self.home.join(META).join("pending.json"),
        )
        .map_err(|e| e.to_string())?;
        let mut touched = Vec::new();
        let outcome = (|| {
            for (i, (relative, replacement)) in changes.iter().enumerate() {
                let target = self.home.join(relative);
                // Detect ordinary edits between backup and publication.
                if before[i] {
                    if !same_tree(&target, &backup.join(format!("before-{i}")))? {
                        return Err(format!("{} changed concurrently", relative.display()));
                    }
                    fs::rename(&target, backup.join(format!("removed-{i}")))
                        .map_err(|e| e.to_string())?;
                } else if exists(&target)? {
                    return Err(format!("{} appeared concurrently", relative.display()));
                }
                touched.push(i);
                if let Some(source) = replacement {
                    fs::rename(source, target).map_err(|e| e.to_string())?;
                }
            }
            validate()
        })();
        if let Err(error) = outcome {
            let mut failures: Vec<String> = Vec::new();
            for i in touched.into_iter().rev() {
                let target = self.home.join(&changes[i].0);
                let result = (|| {
                    if exists(&target)? {
                        if expected_after[i].as_ref() != Some(&signature(&target)?) {
                            return Err(format!(
                                "{} changed during rollback; left untouched",
                                target.display()
                            ));
                        }
                        remove(&target)?;
                    }
                    if before[i] {
                        fs::rename(backup.join(format!("removed-{i}")), &target)
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    failures.push(e);
                }
            }
            if !failures.is_empty() {
                return Err(format!(
                    "{error}; rollback failed: {failures:?}; recovery: {}",
                    backup.display()
                ));
            }
            fs::rename(backup.join("pending.json"), backup.join("rolled-back.json"))
                .map_err(|e| e.to_string())?;
            fs::remove_file(self.home.join(META).join("pending.json"))
                .map_err(|e| e.to_string())?;
            return Err(format!(
                "{error}; rolled back; backup: {}",
                backup.display()
            ));
        }
        fs::rename(backup.join("pending.json"), backup.join("completed.json"))
            .map_err(|e| e.to_string())?;
        fs::remove_file(self.home.join(META).join("pending.json")).map_err(|e| e.to_string())?;
        Ok(backup)
    }
    fn stage(&self) -> Result<tempfile::TempDir> {
        tempfile::Builder::new()
            .prefix("stage-")
            .tempdir_in(self.home.join(META))
            .map_err(|e| e.to_string())
    }
}
fn remove(path: &Path) -> Result<()> {
    if metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|e| e.to_string())
}
fn signature(root: &Path) -> Result<Signature> {
    let sums = tree(root)?
        .into_iter()
        .map(|p| Ok((p.strip_prefix(root).unwrap().to_path_buf(), digest(&p)?)))
        .collect::<Result<_>>()?;
    Ok((metadata(root)?.is_dir(), sums))
}
fn same_tree(a: &Path, b: &Path) -> Result<bool> {
    Ok(signature(a)? == signature(b)?)
}
fn staged_text(stage: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let path = stage.join(name);
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}
fn state_text(disabled: &[String]) -> Result<String> {
    #[derive(Serialize)]
    struct State<'a> {
        disabled: &'a [String],
    }
    toml::to_string(&State { disabled }).map_err(|e| e.to_string())
}
fn validate_config(home: &Path) -> Result<()> {
    Config::load_for_plugin_transaction(home).map(|_| ())
}

/// Enable keeps existing placement; newly referenced applets are appended to the
/// requested section (right by default). Attached applets never get a second icon.
pub fn set_enabled(home: &Path, id: &str, enabled: bool, section: Option<&str>) -> Result<PathBuf> {
    valid_id(id)?;
    if section.is_some_and(|s| !SECTIONS.contains(&s)) {
        return Err("section must be left, center, right or drawer".into());
    }
    let mut session = Session::open(home)?;
    session.observe(&paths(Kind::Applet, id))?;
    let home = &session.home;
    validate_config(home)?;
    directory(&home.join("applets"))?;
    directory(&applets::dir(home, id))?;
    if !exists(&applets::dir(home, id).join("applet.toml"))? {
        return Err("applet is not installed".into());
    }
    let mut disabled = applets::disabled(home)?;
    disabled.retain(|n| n != id);
    if !enabled {
        disabled.push(id.into());
    }
    disabled.sort();
    disabled.dedup();
    let mut doc = bar(home)?;
    let current = modules(&doc);
    if enabled {
        let applet = applets::load(home, id)?;
        if let Some(attach) = &applet.manifest.attach {
            if !(current.contains(attach)
                || attach == "time" && current.iter().any(|m| m == "clock"))
            {
                return Err(format!("add the {attach} module to the bar first"));
            }
            for other in applets::referenced(home, &[&current]).into_iter().flatten() {
                if other.name != id && other.manifest.attach.as_ref() == Some(attach) {
                    return Err(format!(
                        "{} is already attached to {attach}; disable it first",
                        other.name
                    ));
                }
            }
        } else if !current.iter().any(|n| n == id) {
            let section = section.unwrap_or("right");
            if section == "drawer" && !current.iter().any(|n| n == "drawer") {
                return Err("configure a drawer before choosing that section".into());
            }
            if doc.get(section).is_none() {
                doc[section] = toml_edit::value(toml_edit::Array::new());
            }
            doc[section]
                .as_array_mut()
                .ok_or("bar section is not an array")?
                .push(id);
        }
    }
    let stage = session.stage()?;
    let changes = vec![
        (
            PathBuf::from("plugins.toml"),
            Some(staged_text(stage.path(), "state", &state_text(&disabled)?)?),
        ),
        (
            PathBuf::from("bar.toml"),
            Some(staged_text(stage.path(), "bar", &doc.to_string())?),
        ),
    ];
    session.commit(changes, || validate_config(home))
}

fn validate_applet(home: &Path, id: &str) -> Result<()> {
    let applet = applets::load(home, id)?;
    text(&applet.dir.join("view.slint"))?;
    if applet.manifest.provider.is_none() {
        if let Some(command) = &applet.manifest.command {
            if command.is_empty() || command[0].is_empty() {
                return Err("empty provider command".into());
            }
        } else {
            let script = applet
                .manifest
                .script
                .unwrap_or_else(|| format!("{id}.ps1"));
            if script
                .split('/')
                .any(|part| !illium_theme::pack::plain_name(part) || windows_reserved(part))
            {
                return Err("script must be a relative path inside the package".into());
            }
            files::read_config(&applet.dir.join(script))?;
        }
    }
    Ok(())
}
fn prepare(stage: &Path, source: &Path) -> Result<Package> {
    directory(source)?;
    let package: Package =
        toml::from_str(&text(&source.join("plugin.toml"))?).map_err(|e| e.to_string())?;
    package.validate()?;
    let payload = source.join("payload");
    directory(&payload)?;
    // Reject linked or oversized source trees before any copy/decoding.
    tree(&payload)?;
    match package.kind {
        Kind::Applet => {
            mkdir(&stage.join("applets"))?;
            let target = stage.join("applets").join(&package.id);
            copy(&payload, &target)?;
            validate_applet(stage, &package.id)?;
        }
        Kind::Theme => {
            // The existing installer derives its id from the folder name.
            mkdir(&stage.join("source"))?;
            let target = stage.join("source").join(&package.id);
            copy(&payload, &target)?;
            illium_theme::pack::install(stage, &target)?;
        }
    }
    Ok(package)
}
fn merge_manifest(base: &str, local: &str, incoming: &str) -> Result<String> {
    fn merge(
        base: Option<&toml::Value>,
        local: Option<&toml::Value>,
        new: Option<&toml::Value>,
        path: &str,
    ) -> Result<Option<toml::Value>> {
        if local == base {
            return Ok(new.cloned());
        }
        if new == base || local == new {
            return Ok(local.cloned());
        }
        if let (
            Some(toml::Value::Table(b)),
            Some(toml::Value::Table(l)),
            Some(toml::Value::Table(n)),
        ) = (base, local, new)
        {
            let keys: std::collections::BTreeSet<_> =
                b.keys().chain(l.keys()).chain(n.keys()).collect();
            let mut out = toml::map::Map::new();
            for key in keys {
                if let Some(value) =
                    merge(b.get(key), l.get(key), n.get(key), &format!("{path}.{key}"))?
                {
                    out.insert(key.clone(), value);
                }
            }
            return Ok(Some(toml::Value::Table(out)));
        }
        Err(format!(
            "local/upstream manifest conflict at {path}; resolve manually"
        ))
    }
    let parse = |s: &str| toml::from_str::<toml::Value>(s).map_err(|e| e.to_string());
    let merged = merge(
        Some(&parse(base)?),
        Some(&parse(local)?),
        Some(&parse(incoming)?),
        "applet",
    )?
    .ok_or("missing merged manifest")?;
    toml::to_string_pretty(&merged).map_err(|e| e.to_string())
}
/// Install/update only explicit schema-1 local packages. Unknown existing
/// installations are never adopted by matching their names.
pub fn install(home: &Path, source: &Path, update: bool) -> Result<PathBuf> {
    let mut session = Session::open(home)?;
    validate_config(&session.home)?;
    let stage = session.stage()?;
    let package = prepare(stage.path(), source)?;
    session.observe(&paths(package.kind, &package.id))?;
    session.observe(&[record_path(Path::new(""), package.kind, &package.id)])?;
    let home = &session.home;
    let upstream_manifest = if package.kind == Kind::Applet {
        Some(text(
            &stage
                .path()
                .join(format!("applets/{}/applet.toml", package.id)),
        )?)
    } else {
        None
    };
    mkdir(&home.join(package.kind.directory()))?;
    let old = receipt(home, package.kind, &package.id)?;
    if update {
        let old = old
            .as_ref()
            .ok_or("not managed; automatic adoption is not supported")?;
        if package.version == old.package.version {
            return Err("same version; nothing to update".into());
        }
        if package.kind == Kind::Theme && active_theme(home)? == package.id {
            return Err("select another theme before updating".into());
        }
        if package.kind == Kind::Applet && !applets::disabled(home)?.contains(&package.id) {
            return Err("disable the applet before updating".into());
        }
        let current = fingerprints(home, package.kind, &package.id)?;
        let manifest_key = format!("applets/{}/applet.toml", package.id);
        let filter = |map: &BTreeMap<String, String>| {
            map.iter()
                .filter(|(k, _)| package.kind != Kind::Applet || *k != &manifest_key)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<_, _>>()
        };
        if filter(&current) != filter(&old.files) {
            return Err("installed files changed, disappeared or were added; update refused (no files overwritten)".into());
        }
        if package.kind == Kind::Applet {
            let local = text(&home.join(&manifest_key))?;
            let incoming = text(&stage.path().join(&manifest_key))?;
            let base = old
                .applet_manifest
                .as_ref()
                .ok_or("missing manifest baseline")?;
            let merged = merge_manifest(base, &local, &incoming)?;
            fs::write(stage.path().join(&manifest_key), merged).map_err(|e| e.to_string())?;
            validate_applet(stage.path(), &package.id)?;
        }
    } else {
        if package.kind == Kind::Applet
            && fs::read_dir(home.join("applets"))
                .map_err(|e| e.to_string())?
                .count()
                >= applets::MAX_APPLETS
        {
            return Err("applet directory limit reached (64 entries)".into());
        }
        if old.is_some() {
            return Err("already managed; use update".into());
        }
        for path in paths(package.kind, &package.id) {
            if exists(&home.join(path))? {
                return Err(
                    "already installed; existing installations are not adopted or overwritten"
                        .into(),
                );
            }
        }
    }
    let record = Receipt {
        source: fs::canonicalize(source)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned(),
        files: fingerprints(stage.path(), package.kind, &package.id)?,
        applet_manifest: upstream_manifest,
        package: package.clone(),
    };
    let mut changes = Vec::new();
    if !update && package.kind == Kind::Applet {
        let mut disabled = applets::disabled(home)?;
        if !disabled.contains(&package.id) {
            disabled.push(package.id.clone());
        }
        changes.push((
            PathBuf::from("plugins.toml"),
            Some(staged_text(stage.path(), "state", &state_text(&disabled)?)?),
        ));
    }
    for path in paths(package.kind, &package.id) {
        changes.push((path.clone(), Some(stage.path().join(path))));
    }
    let record_relative = record_path(Path::new(""), package.kind, &package.id);
    let record_json = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    if record_json.len() > 256 * 1024 {
        return Err("receipt exceeds 256 KiB".into());
    }
    changes.push((
        record_relative,
        Some(staged_text(stage.path(), "receipt", &record_json)?),
    ));
    session.commit(changes, || validate_config(home))
}

pub fn uninstall(home: &Path, kind: Kind, id: &str) -> Result<PathBuf> {
    valid_id(id)?;
    if kind.bundled(id) {
        return Err("built-ins cannot be uninstalled; disable the applet instead".into());
    }
    let mut session = Session::open(home)?;
    session.observe(&paths(kind, id))?;
    session.observe(&[record_path(Path::new(""), kind, id)])?;
    let home = &session.home;
    validate_config(home)?;
    directory(&home.join(kind.directory()))?;
    let r = receipt(home, kind, id)?
        .ok_or("not managed; refusing to delete an existing installation")?;
    if kind == Kind::Theme && active_theme(home)? == id {
        return Err("select another theme before uninstalling".into());
    }
    if kind == Kind::Applet && !applets::disabled(home)?.iter().any(|n| n == id) {
        return Err("disable the applet before uninstalling".into());
    }
    // Unlike updates, removal refuses modified manifests too: preserve every
    // local edit until the user has explicitly resolved/exported it.
    if fingerprints(home, kind, id)? != r.files {
        return Err("local files changed; uninstall refused; preserve/resolve edits first".into());
    }
    let stage = session.stage()?;
    let mut changes = Vec::new();
    if kind == Kind::Applet {
        let mut doc = bar(home)?;
        for section in SECTIONS {
            if let Some(array) = doc.get_mut(section).and_then(|v| v.as_array_mut()) {
                array.retain(|v| v.as_str() != Some(id));
            }
        }
        if doc
            .get("drawer")
            .and_then(|v| v.as_array())
            .is_some_and(|a| a.is_empty())
        {
            for section in ["left", "center", "right"] {
                if let Some(a) = doc.get_mut(section).and_then(|v| v.as_array_mut()) {
                    a.retain(|v| v.as_str() != Some("drawer"));
                }
            }
        }
        changes.push((
            PathBuf::from("bar.toml"),
            Some(staged_text(stage.path(), "bar", &doc.to_string())?),
        ));
        let mut disabled = applets::disabled(home)?;
        disabled.retain(|n| n != id);
        changes.push((
            PathBuf::from("plugins.toml"),
            Some(staged_text(stage.path(), "state", &state_text(&disabled)?)?),
        ));
    }
    for path in paths(kind, id) {
        changes.push((path, None));
    }
    changes.push((record_path(Path::new(""), kind, id), None));
    session.commit(changes, || validate_config(home))
}

#[cfg(test)]
mod tests;
