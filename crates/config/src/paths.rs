//! Where the CLI keeps its files.
//!
//! Historically everything lived in `~/.atlassian-cli/` with no way to move it:
//! `--config` pointed at a different config *file*, but the two credential files
//! had no flag and no environment variable. Anyone keeping their tools under
//! `~/.config` needed a symlink.
//!
//! Resolution order:
//!
//! 1. `$ATLASSIAN_CLI_CONFIG_DIR` (or `--config-dir`)
//! 2. `$XDG_CONFIG_HOME/atlassian-cli`
//! 3. `~/.config/atlassian-cli`
//! 4. the legacy `~/.atlassian-cli`, then `~/.atlcli`, if either still holds our files
//!
//! `~/.config` is used on macOS too. `dirs::config_dir()` would give
//! `~/Library/Application Support` there, which is not what someone asking for
//! XDG means.
//!
//! All three files live in whichever directory wins, so one setting moves
//! everything.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

pub const CONFIG_DIR_ENV: &str = "ATLASSIAN_CLI_CONFIG_DIR";
pub const XDG_CONFIG_HOME_ENV: &str = "XDG_CONFIG_HOME";

/// Directory name under the XDG config root. No leading dot: it is already hidden
/// by its parent.
pub const APP_DIR: &str = "atlassian-cli";

/// Older locations, newest first. `.atlcli` predates `.atlassian-cli`.
pub const LEGACY_DIRS: [&str; 2] = [".atlassian-cli", ".atlcli"];

pub const CONFIG_FILENAME: &str = "config.yaml";
pub const CREDENTIALS_FILENAME: &str = "credentials";
pub const ENCRYPTED_FILENAME: &str = "credentials.enc";

/// Every file we own. A directory counts as ours if it holds at least one.
pub const OWNED_FILES: [&str; 3] = [CONFIG_FILENAME, CREDENTIALS_FILENAME, ENCRYPTED_FILENAME];

/// Suffix for a legacy directory that has been migrated away from.
pub const ARCHIVE_SUFFIX: &str = ".migrated";

/// Which rule picked the directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigDirSource {
    /// `$ATLASSIAN_CLI_CONFIG_DIR` or `--config-dir`. Never migrated: an explicit
    /// choice is not something to second-guess.
    Explicit,
    /// `$XDG_CONFIG_HOME/atlassian-cli`.
    Xdg,
    /// `~/.config/atlassian-cli`, or the platform equivalent on Windows.
    Default,
    /// A legacy directory that still holds our files.
    Legacy,
}

/// The inputs resolution reads.
///
/// Built from the process in `from_process`, and constructed literally in tests.
/// Keeping the environment out of the resolution itself is what lets the tests
/// run in parallel without a lock around `set_var`.
#[derive(Debug, Clone, Default)]
pub struct PathEnv {
    pub explicit_dir: Option<String>,
    pub xdg_config_home: Option<String>,
    pub home: Option<PathBuf>,
    /// Windows only: `%LOCALAPPDATA%`. Deliberately the *local* one, not roaming;
    /// see `default_base`.
    pub native_config: Option<PathBuf>,
}

impl PathEnv {
    pub fn from_process() -> Self {
        Self {
            explicit_dir: std::env::var(CONFIG_DIR_ENV).ok(),
            xdg_config_home: std::env::var(XDG_CONFIG_HOME_ENV).ok(),
            home: dirs::home_dir(),
            native_config: dirs::config_local_dir(),
        }
    }
}

/// The resolved location of every file the CLI owns.
#[derive(Debug, Clone)]
pub struct ConfigPaths {
    dir: PathBuf,
    /// Where we would like to be. Differs from `dir` only when a legacy
    /// directory won, in which case this is the migration target.
    preferred: PathBuf,
    source: ConfigDirSource,
    /// From `--config`, which moves the config file alone.
    config_file_override: Option<PathBuf>,
}

fn non_empty(value: Option<&String>) -> Option<&str> {
    value.map(|v| v.trim()).filter(|v| !v.is_empty())
}

/// The base directory to put `atlassian-cli/` under when nothing is set.
fn default_base(env: &PathEnv) -> Option<PathBuf> {
    if cfg!(windows) {
        // %LOCALAPPDATA%, not %APPDATA%. The encryption key derives from the
        // machine id, so a roaming profile would replicate a credentials file
        // that the second machine cannot decrypt.
        env.native_config.clone()
    } else {
        env.home.as_ref().map(|h| h.join(".config"))
    }
}

impl ConfigPaths {
    /// Resolve from the process environment and the filesystem.
    pub fn resolve() -> Result<Self> {
        Self::resolve_with(&PathEnv::from_process(), &is_populated)
    }

    /// Resolve from an explicit override, falling back to the environment.
    pub fn resolve_from(explicit: Option<PathBuf>) -> Result<Self> {
        let mut env = PathEnv::from_process();
        if let Some(dir) = explicit {
            env.explicit_dir = Some(dir.to_string_lossy().into_owned());
        }
        Self::resolve_with(&env, &is_populated)
    }

    /// The resolution rules. `populated` is the only contact with the filesystem,
    /// so tests can supply their own.
    pub fn resolve_with(env: &PathEnv, populated: &dyn Fn(&Path) -> bool) -> Result<Self> {
        // 1. An explicit directory wins outright, and is used verbatim. A
        //    relative value is honoured here, unlike XDG below: this is our own
        //    variable, and `ATLASSIAN_CLI_CONFIG_DIR=./ci-config` is a
        //    reasonable thing to write in a job with a fixed working directory.
        if let Some(explicit) = non_empty(env.explicit_dir.as_ref()) {
            let dir = expand_home(Path::new(explicit), env.home.as_deref());
            return Ok(Self::at(dir, ConfigDirSource::Explicit));
        }

        // 2/3. The XDG location, or the platform default.
        //      A relative $XDG_CONFIG_HOME is ignored, as the basedir spec says
        //      and as dirs-sys does. Resolving it against the current directory
        //      would drop a credentials file into whatever repository the user
        //      happens to be standing in.
        let (base, source) = match non_empty(env.xdg_config_home.as_ref())
            .map(Path::new)
            .filter(|p| p.is_absolute())
        {
            Some(xdg) => (xdg.to_path_buf(), ConfigDirSource::Xdg),
            None => match default_base(env) {
                Some(base) => (base, ConfigDirSource::Default),
                None => {
                    return Err(anyhow!(
                        "Cannot determine your home directory. \
                         Set {CONFIG_DIR_ENV} or pass --config-dir to choose where \
                         atlassian-cli should keep its files."
                    ))
                }
            },
        };
        let preferred = base.join(APP_DIR);

        // The preferred location already holds our files: nothing to consider.
        if populated(&preferred) {
            return Ok(Self::at(preferred, source));
        }

        // 4. A legacy directory, but only if it actually holds something. A
        //    leftover empty ~/.atlassian-cli must not pin someone on the old
        //    location forever, nor trigger a migration that copies nothing and
        //    then renames a directory they may have created deliberately.
        if let Some(home) = env.home.as_deref() {
            for legacy in LEGACY_DIRS {
                let candidate = home.join(legacy);
                if populated(&candidate) {
                    return Ok(Self {
                        dir: candidate,
                        preferred,
                        source: ConfigDirSource::Legacy,
                        config_file_override: None,
                    });
                }
            }
        }

        // Fresh install: land in the preferred location.
        Ok(Self::at(preferred, source))
    }

    fn at(dir: PathBuf, source: ConfigDirSource) -> Self {
        Self {
            preferred: dir.clone(),
            dir,
            source,
            config_file_override: None,
        }
    }

    /// Use this directory as-is. For tests and embedding.
    pub fn for_dir(dir: impl Into<PathBuf>) -> Self {
        Self::at(dir.into(), ConfigDirSource::Explicit)
    }

    /// For tests that need to exercise the migration path.
    #[doc(hidden)]
    pub fn for_legacy(dir: impl Into<PathBuf>, preferred: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            preferred: preferred.into(),
            source: ConfigDirSource::Legacy,
            config_file_override: None,
        }
    }

    /// Point the config file somewhere else. Affects `config_file` only: the
    /// credentials stay in the resolved directory.
    pub fn with_config_file_override(mut self, file: Option<PathBuf>) -> Self {
        self.config_file_override = file;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn preferred(&self) -> &Path {
        &self.preferred
    }

    pub fn source(&self) -> ConfigDirSource {
        self.source
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_file_override
            .clone()
            .unwrap_or_else(|| self.dir.join(CONFIG_FILENAME))
    }

    pub fn credentials_file(&self) -> PathBuf {
        self.dir.join(CREDENTIALS_FILENAME)
    }

    pub fn encrypted_file(&self) -> PathBuf {
        self.dir.join(ENCRYPTED_FILENAME)
    }
}

/// Does this directory hold at least one of our files?
pub fn is_populated(dir: &Path) -> bool {
    OWNED_FILES.iter().any(|name| dir.join(name).exists())
}

/// Expand a leading `~` against the home directory.
fn expand_home(path: &Path, home: Option<&Path>) -> PathBuf {
    let (Some(home), Some(text)) = (home, path.to_str()) else {
        return path.to_path_buf();
    };
    match text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        Some(rest) => home.join(rest),
        None if text == "~" => home.to_path_buf(),
        None => path.to_path_buf(),
    }
}

/// Outcome of moving a legacy directory to the preferred location.
#[derive(Debug)]
pub enum DirMigration {
    NotNeeded,
    Migrated {
        from: PathBuf,
        to: PathBuf,
        files: Vec<&'static str>,
        /// Where the old directory was renamed to. `None` if the rename failed,
        /// which is untidy but not a correctness problem.
        archived: Option<PathBuf>,
        /// A plaintext `credentials` file was scrubbed from the archive after
        /// its contents were copied to the new location. Worth telling the user
        /// about: it is the one thing migration removes.
        scrubbed_plaintext: bool,
    },
    /// Nothing was copied, and the returned paths still point at `from`.
    Failed {
        from: PathBuf,
        to: PathBuf,
        error: String,
    },
}

/// Move a legacy directory to the preferred location, then rename the original.
///
/// Files are copied into a staging directory and promoted with a single rename,
/// rather than written into the target one at a time. A half-populated target
/// would be worse than no migration: the next run's resolver would see files in
/// the new location, choose it, and the user would appear logged out while their
/// tokens sat in a directory the CLI no longer reads.
///
/// The original is renamed to `<name>.migrated` rather than left in place. A
/// lingering copy that is silently ignored is a trap: anyone editing it later is
/// editing a dead file. Nothing is deleted.
///
/// Returns the paths to actually use. On failure that is the legacy directory,
/// so someone whose new location is unwritable keeps working.
pub fn migrate_legacy_dir(paths: ConfigPaths) -> (ConfigPaths, DirMigration) {
    if paths.source() != ConfigDirSource::Legacy {
        return (paths, DirMigration::NotNeeded);
    }

    let from = paths.dir().to_path_buf();
    let to = paths.preferred().to_path_buf();

    // A pre-existing symlink from the old path to the new one is a workaround
    // people already use. Comparing canonical paths is what catches it.
    if let (Ok(a), Ok(b)) = (from.canonicalize(), to.canonicalize()) {
        if a == b {
            return (paths, DirMigration::NotNeeded);
        }
    }

    // Never merge into a directory that already holds something.
    if is_populated(&to) {
        return (paths, DirMigration::NotNeeded);
    }

    match stage_and_promote(&from, &to) {
        Ok(files) => {
            let archived = archive_legacy_dir(&from);
            // The copy in the new location is now the only one that should
            // exist. Leaving the archived plaintext token behind would be a
            // regression against the pre-migration behaviour, where the next
            // `auth login` shredded it: the file would sit in
            // `~/.atlassian-cli.migrated/credentials` indefinitely, readable,
            // while the user believed encryption had taken over.
            let scrubbed_plaintext = files.contains(&CREDENTIALS_FILENAME)
                && scrub_file(
                    &archived
                        .clone()
                        .unwrap_or_else(|| from.clone())
                        .join(CREDENTIALS_FILENAME),
                );
            let migrated = ConfigPaths {
                dir: to.clone(),
                preferred: to.clone(),
                source: ConfigDirSource::Default,
                config_file_override: paths.config_file_override.clone(),
            };
            (
                migrated,
                DirMigration::Migrated {
                    from,
                    to,
                    files,
                    archived,
                    scrubbed_plaintext,
                },
            )
        }
        Err(error) => (
            paths,
            DirMigration::Failed {
                from,
                to,
                error: error.to_string(),
            },
        ),
    }
}

fn stage_and_promote(from: &Path, to: &Path) -> Result<Vec<&'static str>> {
    let parent = to
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", to.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| anyhow!("Unable to create {}: {}", parent.display(), e))?;

    let staging = parent.join(format!("{APP_DIR}.migrating.{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    create_private_dir(&staging)?;

    let mut copied = Vec::new();
    for name in OWNED_FILES {
        let src = from.join(name);
        if !src.exists() {
            continue;
        }
        let result = std::fs::read(&src)
            .map_err(|e| anyhow!("Unable to read {}: {}", src.display(), e))
            .and_then(|bytes| write_private(&staging.join(name), &bytes));
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
        copied.push(name);
    }

    // One atomic step. If a concurrent invocation won the race, the target is
    // no longer empty and there is nothing left to do.
    if let Err(e) = std::fs::rename(&staging, to) {
        if is_populated(to) {
            let _ = std::fs::remove_dir_all(&staging);
            return Ok(copied);
        }
        // `to` holds none of our files but is not a name we can rename onto:
        // an existing directory with a `.DS_Store`, a `.keep`, or an unrelated
        // file in it (ENOTEMPTY), or a symlink to a directory, which is exactly
        // the shape a stow/chezmoi dotfile setup has - and dotfile users are
        // who asked for this feature. Renaming a directory onto either fails
        // forever, so without this fallback the migration would never complete
        // and every single command would print the same warning.
        let result = promote_each_file(&staging, to, &copied);
        let _ = std::fs::remove_dir_all(&staging);
        return match result {
            Ok(()) => Ok(copied),
            Err(fallback) => Err(anyhow!(
                "Unable to move {} into place: {} (and moving the files \
                 individually failed: {})",
                to.display(),
                e,
                fallback
            )),
        };
    }

    Ok(copied)
}

/// Move each staged file into an existing target directory, one atomic rename
/// apiece, undoing them all if any fails.
///
/// The all-or-nothing part is the point. A target holding `config.yaml` but not
/// `credentials.enc` is worse than an unmigrated one: the resolver would pick it
/// on the next run and the user would look logged out while their token sat in a
/// directory the CLI had stopped reading.
fn promote_each_file(staging: &Path, to: &Path, files: &[&'static str]) -> Result<()> {
    create_private_dir(to)?;

    let mut moved: Vec<&'static str> = Vec::new();
    for name in files {
        match std::fs::rename(staging.join(name), to.join(name)) {
            Ok(()) => moved.push(name),
            Err(e) => {
                for done in &moved {
                    let _ = std::fs::rename(to.join(done), staging.join(done));
                }
                return Err(anyhow!(
                    "Unable to write {}: {}",
                    to.join(name).display(),
                    e
                ));
            }
        }
    }

    Ok(())
}

/// Overwrite a file's bytes before removing it, and report whether it went.
///
/// The same one-pass overwrite the auth crate uses: it defeats an undelete on
/// the filesystems people actually run, and claims nothing more than that. On a
/// copy-on-write filesystem the old blocks may survive regardless.
///
/// `symlink_metadata`, so a symlinked `credentials` is not written through. That
/// happens: pointing it at a dotfiles repository is one of the workarounds this
/// feature replaces, and zeroing 21 bytes inside someone's git working tree
/// because they migrated is not ours to do. The link itself is removed, and the
/// caller reports nothing scrubbed, because the plaintext still exists at the
/// far end of it.
fn scrub_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };

    if metadata.is_symlink() {
        let _ = std::fs::remove_file(path);
        return false;
    }
    if !metadata.is_file() {
        return false;
    }

    let _ = std::fs::write(path, vec![0u8; metadata.len() as usize]);
    std::fs::remove_file(path).is_ok()
}

/// Rename the legacy directory aside so it cannot be edited by mistake.
fn archive_legacy_dir(from: &Path) -> Option<PathBuf> {
    let name = from.file_name()?.to_str()?;
    for attempt in 0..10 {
        let suffix = if attempt == 0 {
            ARCHIVE_SUFFIX.to_string()
        } else {
            format!("{ARCHIVE_SUFFIX}.{attempt}")
        };
        let candidate = from.with_file_name(format!("{name}{suffix}"));
        if candidate.exists() {
            continue;
        }
        if std::fs::rename(from, &candidate).is_ok() {
            return Some(candidate);
        }
        return None;
    }
    None
}

/// Write a file only its owner can read. See the auth crate for the same helper;
/// duplicated rather than shared to avoid a dependency edge between two sibling
/// crates for twelve lines.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.tmp{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));

    {
        // `create_new`, not `create`: the temp name is derived from the pid, so
        // it is guessable, and opening an existing path would follow a symlink
        // planted there and write a token wherever it points. A leftover file
        // means a crash, and is cleared only once confirmed to be a plain file.
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = match options.open(&tmp) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = std::fs::symlink_metadata(&tmp)
                    .map(|m| m.is_file())
                    .unwrap_or(false);
                if !stale {
                    return Err(anyhow!(
                        "Refusing to write {}: it exists and is not a regular file",
                        tmp.display()
                    ));
                }
                std::fs::remove_file(&tmp)
                    .map_err(|e| anyhow!("Unable to clear stale {}: {}", tmp.display(), e))?;
                options
                    .open(&tmp)
                    .map_err(|e| anyhow!("Unable to write {}: {}", tmp.display(), e))?
            }
            Err(e) => return Err(anyhow!("Unable to write {}: {}", tmp.display(), e)),
        };
        file.write_all(bytes)
            .map_err(|e| anyhow!("Unable to write {}: {}", tmp.display(), e))?;
        file.sync_all().ok();
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow!("Unable to write {}: {}", path.display(), e)
    })?;

    Ok(())
}

/// Create a directory only its owner can enter.
pub fn create_private_dir(dir: &Path) -> Result<()> {
    // Only a directory we create is ours to set a mode on. Tightening one that
    // already exists would mean `--config /shared/team.yaml` silently chmodding
    // /shared, or `ATLASSIAN_CLI_CONFIG_DIR=$HOME` chmodding the home directory.
    if dir.is_dir() {
        return Ok(());
    }

    std::fs::create_dir_all(dir)
        .map_err(|e| anyhow!("Unable to create directory {}: {}", dir.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| anyhow!("Unable to restrict {}: {}", dir.display(), e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Resolution with a stated set of populated directories, so nothing touches
    /// the filesystem or the process environment.
    fn resolve(env: PathEnv, populated: &[&str]) -> Result<ConfigPaths> {
        let set: HashSet<PathBuf> = populated.iter().map(PathBuf::from).collect();
        ConfigPaths::resolve_with(&env, &move |p: &Path| set.contains(p))
    }

    fn env() -> PathEnv {
        PathEnv {
            home: Some(PathBuf::from("/home/u")),
            ..Default::default()
        }
    }

    #[test]
    fn explicit_wins_over_everything() {
        let paths = resolve(
            PathEnv {
                explicit_dir: Some("/explicit".into()),
                xdg_config_home: Some("/xdg".into()),
                ..env()
            },
            &["/xdg/atlassian-cli", "/home/u/.atlassian-cli"],
        )
        .unwrap();

        assert_eq!(paths.dir(), Path::new("/explicit"));
        assert_eq!(paths.source(), ConfigDirSource::Explicit);
    }

    #[test]
    fn blank_explicit_is_treated_as_unset() {
        for blank in ["", "   "] {
            let paths = resolve(
                PathEnv {
                    explicit_dir: Some(blank.into()),
                    ..env()
                },
                &[],
            )
            .unwrap();
            assert_eq!(paths.dir(), Path::new("/home/u/.config/atlassian-cli"));
        }
    }

    /// Our own variable, so a relative value is honoured: CI jobs with a fixed
    /// working directory reasonably write `./ci-config`.
    #[test]
    fn relative_explicit_is_honoured() {
        let paths = resolve(
            PathEnv {
                explicit_dir: Some("ci-config".into()),
                ..env()
            },
            &[],
        )
        .unwrap();
        assert_eq!(paths.dir(), Path::new("ci-config"));
    }

    #[test]
    fn explicit_expands_a_leading_tilde() {
        let paths = resolve(
            PathEnv {
                explicit_dir: Some("~/elsewhere".into()),
                ..env()
            },
            &[],
        )
        .unwrap();
        assert_eq!(paths.dir(), Path::new("/home/u/elsewhere"));
    }

    #[test]
    fn xdg_config_home_is_used_when_absolute() {
        let paths = resolve(
            PathEnv {
                xdg_config_home: Some("/xdg".into()),
                ..env()
            },
            &[],
        )
        .unwrap();
        assert_eq!(paths.dir(), Path::new("/xdg/atlassian-cli"));
        assert_eq!(paths.source(), ConfigDirSource::Xdg);
    }

    /// The basedir spec says a relative value must be ignored. Resolving it
    /// against the cwd would write credentials into whatever repo you are in.
    #[test]
    fn relative_xdg_config_home_is_ignored() {
        for value in ["relative/path", ""] {
            let paths = resolve(
                PathEnv {
                    xdg_config_home: Some(value.into()),
                    ..env()
                },
                &[],
            )
            .unwrap();
            assert_eq!(paths.dir(), Path::new("/home/u/.config/atlassian-cli"));
            assert_eq!(paths.source(), ConfigDirSource::Default);
        }
    }

    #[test]
    fn a_populated_legacy_directory_is_used() {
        let paths = resolve(env(), &["/home/u/.atlassian-cli"]).unwrap();
        assert_eq!(paths.dir(), Path::new("/home/u/.atlassian-cli"));
        assert_eq!(paths.source(), ConfigDirSource::Legacy);
        assert_eq!(
            paths.preferred(),
            Path::new("/home/u/.config/atlassian-cli"),
            "the migration target should be the preferred location"
        );
    }

    /// A leftover empty directory must not pin anyone on the old location.
    #[test]
    fn an_empty_legacy_directory_is_ignored() {
        let paths = resolve(env(), &[]).unwrap();
        assert_eq!(paths.dir(), Path::new("/home/u/.config/atlassian-cli"));
        assert_eq!(paths.source(), ConfigDirSource::Default);
    }

    #[test]
    fn the_older_atlcli_directory_is_still_found() {
        let paths = resolve(env(), &["/home/u/.atlcli"]).unwrap();
        assert_eq!(paths.dir(), Path::new("/home/u/.atlcli"));
        assert_eq!(paths.source(), ConfigDirSource::Legacy);
    }

    #[test]
    fn the_newer_legacy_directory_wins_when_both_exist() {
        let paths = resolve(env(), &["/home/u/.atlassian-cli", "/home/u/.atlcli"]).unwrap();
        assert_eq!(paths.dir(), Path::new("/home/u/.atlassian-cli"));
    }

    /// An already-migrated install: the new location has files, so the legacy
    /// one is never consulted.
    #[test]
    fn a_populated_preferred_directory_beats_legacy() {
        let paths = resolve(
            env(),
            &["/home/u/.config/atlassian-cli", "/home/u/.atlassian-cli"],
        )
        .unwrap();
        assert_eq!(paths.dir(), Path::new("/home/u/.config/atlassian-cli"));
        assert_eq!(paths.source(), ConfigDirSource::Default);
    }

    #[test]
    fn a_fresh_install_lands_in_the_preferred_directory() {
        let paths = resolve(env(), &[]).unwrap();
        assert_eq!(paths.dir(), Path::new("/home/u/.config/atlassian-cli"));
    }

    /// Previously this fell back to the current directory, which under a single
    /// shared directory would write credentials.enc into whatever repo you are in.
    #[test]
    fn no_home_and_no_override_is_an_error_naming_the_variable() {
        let err = resolve(PathEnv::default(), &[]).unwrap_err().to_string();
        assert!(err.contains(CONFIG_DIR_ENV), "got: {err}");
    }

    #[test]
    fn no_home_is_fine_when_an_explicit_directory_is_given() {
        let paths = resolve(
            PathEnv {
                explicit_dir: Some("/explicit".into()),
                ..Default::default()
            },
            &[],
        )
        .unwrap();
        assert_eq!(paths.dir(), Path::new("/explicit"));
    }

    /// `--config` moves the config file alone; credentials stay put.
    #[test]
    fn the_config_file_override_does_not_move_the_credentials() {
        let paths = resolve(
            PathEnv {
                explicit_dir: Some("/dir".into()),
                ..env()
            },
            &[],
        )
        .unwrap()
        .with_config_file_override(Some(PathBuf::from("/elsewhere/custom.yaml")));

        assert_eq!(paths.config_file(), Path::new("/elsewhere/custom.yaml"));
        assert_eq!(paths.credentials_file(), Path::new("/dir/credentials"));
        assert_eq!(paths.encrypted_file(), Path::new("/dir/credentials.enc"));
    }

    #[test]
    fn file_names_hang_off_the_resolved_directory() {
        let paths = ConfigPaths::for_dir("/dir");
        assert_eq!(paths.config_file(), Path::new("/dir/config.yaml"));
        assert_eq!(paths.credentials_file(), Path::new("/dir/credentials"));
        assert_eq!(paths.encrypted_file(), Path::new("/dir/credentials.enc"));
    }

    // ---------------------------------------------------------------------
    // Migration
    // ---------------------------------------------------------------------

    use tempfile::TempDir;

    fn seed(dir: &Path, files: &[(&str, &str)]) {
        std::fs::create_dir_all(dir).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
    }

    #[test]
    fn migration_copies_files_and_archives_the_original() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(
            &from,
            &[("config.yaml", "profiles: {}"), ("credentials.enc", "{}")],
        );

        let (paths, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        match outcome {
            DirMigration::Migrated {
                files, archived, ..
            } => {
                assert_eq!(files, vec!["config.yaml", "credentials.enc"]);
                let archived = archived.expect("the original should have been renamed");
                assert!(archived.exists());
                assert_eq!(archived.file_name().unwrap(), ".atlassian-cli.migrated");
            }
            other => panic!("expected a migration, got {other:?}"),
        }

        assert_eq!(
            paths.dir(),
            to,
            "the caller should be pointed at the new location"
        );
        assert_eq!(
            std::fs::read_to_string(to.join("config.yaml")).unwrap(),
            "profiles: {}"
        );
        assert!(!from.exists(), "the original path should no longer resolve");
    }

    /// The archive must not keep a second readable copy of a plaintext token.
    ///
    /// Before the directory moved, `auth login` shredded the one plaintext
    /// `credentials` file it found. Copying it aside and leaving it there would
    /// quietly undo that: the user encrypts their token and a cleartext copy
    /// lives on in `~/.atlassian-cli.migrated` for as long as they never look.
    #[test]
    fn migration_does_not_strand_a_plaintext_token_in_the_archive() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(
            &from,
            &[("config.yaml", "profiles: {}"), ("credentials", "w=secret")],
        );

        let (_, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        let (archived, scrubbed) = match outcome {
            DirMigration::Migrated {
                archived,
                scrubbed_plaintext,
                ..
            } => (
                archived.expect("the original should be renamed"),
                scrubbed_plaintext,
            ),
            other => panic!("expected a migration, got {other:?}"),
        };

        assert!(scrubbed, "the user must be told the copy was removed");
        assert!(
            !archived.join("credentials").exists(),
            "the archived plaintext token should be gone"
        );
        assert!(
            archived.join("config.yaml").exists(),
            "only the secret is removed; the rest of the archive stays"
        );
        assert_eq!(
            std::fs::read_to_string(to.join("credentials")).unwrap(),
            "w=secret",
            "the surviving copy must be the one at the new location"
        );
    }

    /// A symlinked `credentials` is not written through.
    ///
    /// Pointing it at a dotfiles repository is one of the workarounds this
    /// feature exists to replace, so the file at the far end is git-managed and
    /// belongs to the user. The link goes; its target is left exactly as it was,
    /// and nothing is reported as scrubbed, since the plaintext still exists.
    #[cfg(unix)]
    #[test]
    fn migration_does_not_scrub_through_a_symlinked_credentials_file() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        let real = home.path().join("dotfiles").join("credentials");
        seed(&from, &[("config.yaml", "profiles: {}")]);
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, "w=secret").unwrap();
        std::os::unix::fs::symlink(&real, from.join("credentials")).unwrap();

        let (_, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        let (archived, scrubbed) = match outcome {
            DirMigration::Migrated {
                archived,
                scrubbed_plaintext,
                ..
            } => (
                archived.expect("the original should be renamed"),
                scrubbed_plaintext,
            ),
            other => panic!("expected a migration, got {other:?}"),
        };

        assert_eq!(
            std::fs::read_to_string(&real).unwrap(),
            "w=secret",
            "the user's own file must not be zeroed"
        );
        assert!(
            !scrubbed,
            "nothing was destroyed, so the notice must not say so"
        );
        assert!(
            !archived.join("credentials").exists(),
            "the dangling link should not be left in the archive"
        );
        assert_eq!(
            std::fs::read_to_string(to.join("credentials")).unwrap(),
            "w=secret",
            "the copy is taken through the link, as before"
        );
    }

    /// An encrypted-only install has no plaintext file, so nothing is removed
    /// and the notice must not claim otherwise.
    #[test]
    fn migration_reports_no_scrub_when_there_was_no_plaintext_file() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(&from, &[("credentials.enc", "{}")]);

        let (_, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        match outcome {
            DirMigration::Migrated {
                scrubbed_plaintext, ..
            } => assert!(!scrubbed_plaintext),
            other => panic!("expected a migration, got {other:?}"),
        }
    }

    /// A target directory that exists but holds none of our files.
    ///
    /// `.DS_Store`, a `.keep`, or anything else a dotfile manager drops there
    /// makes `rename(staging, to)` fail with ENOTEMPTY every single run. The
    /// files must be promoted individually instead of the migration warning
    /// forever.
    #[test]
    fn migration_into_a_non_empty_but_unowned_target() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(
            &from,
            &[("config.yaml", "profiles: {}"), ("credentials.enc", "{}")],
        );
        seed(&to, &[(".DS_Store", "junk")]);

        let (paths, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        assert!(
            matches!(outcome, DirMigration::Migrated { .. }),
            "expected a migration, got {outcome:?}"
        );
        assert_eq!(paths.dir(), to);
        assert_eq!(
            std::fs::read_to_string(to.join("config.yaml")).unwrap(),
            "profiles: {}"
        );
        assert!(to.join("credentials.enc").exists());
        assert!(
            to.join(".DS_Store").exists(),
            "the unrelated file must survive"
        );
        assert!(!from.exists());
        assert!(
            std::fs::read_dir(to.parent().unwrap())
                .unwrap()
                .filter_map(|e| e.ok())
                .all(|e| !e.file_name().to_string_lossy().contains(".migrating.")),
            "the staging directory must not be left behind"
        );
    }

    /// A symlinked config directory, which is how the people who asked for this
    /// feature already work around its absence.
    #[cfg(unix)]
    #[test]
    fn migration_into_a_symlinked_target_keeps_the_symlink() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let real = home.path().join("dotfiles").join("atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(&from, &[("config.yaml", "profiles: {}")]);
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&real, &to).unwrap();

        let (paths, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        assert!(
            matches!(outcome, DirMigration::Migrated { .. }),
            "expected a migration, got {outcome:?}"
        );
        assert_eq!(paths.dir(), to);
        assert!(
            std::fs::symlink_metadata(&to).unwrap().is_symlink(),
            "the symlink must not be replaced by a real directory"
        );
        assert_eq!(
            std::fs::read_to_string(real.join("config.yaml")).unwrap(),
            "profiles: {}",
            "the file should land in the directory the symlink points at"
        );
    }

    /// Only what is there. An install with no credentials must not create empty ones.
    #[test]
    fn migration_copies_only_the_files_that_exist() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlcli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(&from, &[("config.yaml", "x")]);

        let (_, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        match outcome {
            DirMigration::Migrated { files, .. } => assert_eq!(files, vec!["config.yaml"]),
            other => panic!("expected a migration, got {other:?}"),
        }
        assert!(!to.join("credentials.enc").exists());
    }

    #[test]
    fn migration_is_idempotent() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(&from, &[("config.yaml", "x")]);

        migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        // Second run: the source is gone, so resolution would never call this
        // again, but calling it must still be harmless.
        let (_, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));
        assert!(matches!(outcome, DirMigration::NotNeeded));
        assert!(!home.path().join(".atlassian-cli.migrated.1").exists());
    }

    /// Never merge into a live directory.
    #[test]
    fn migration_refuses_a_populated_target() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(&from, &[("config.yaml", "old")]);
        seed(&to, &[("config.yaml", "new")]);

        let (paths, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        assert!(matches!(outcome, DirMigration::NotNeeded));
        assert_eq!(paths.dir(), from);
        assert_eq!(
            std::fs::read_to_string(to.join("config.yaml")).unwrap(),
            "new"
        );
    }

    /// The symlink workaround people already use.
    #[cfg(unix)]
    #[test]
    fn migration_notices_the_two_paths_are_the_same_directory() {
        let home = TempDir::new().unwrap();
        let to = home.path().join(".config").join(APP_DIR);
        seed(&to, &[("config.yaml", "x")]);
        let from = home.path().join(".atlassian-cli");
        std::os::unix::fs::symlink(&to, &from).unwrap();

        let (paths, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        assert!(matches!(outcome, DirMigration::NotNeeded));
        assert!(from.exists(), "the symlink must not be archived");
        assert_eq!(paths.dir(), from);
    }

    #[test]
    fn migration_picks_another_name_when_the_archive_exists() {
        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(&from, &[("config.yaml", "x")]);
        seed(
            &home.path().join(".atlassian-cli.migrated"),
            &[("stale", "x")],
        );

        let (_, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        match outcome {
            DirMigration::Migrated { archived, .. } => {
                let archived = archived.expect("should still archive");
                assert_eq!(archived.file_name().unwrap(), ".atlassian-cli.migrated.1");
            }
            other => panic!("expected a migration, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn migrated_files_and_directory_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        let to = home.path().join(".config").join(APP_DIR);
        seed(&from, &[("credentials.enc", "{}")]);
        // Loosen the source, so we are testing what we write rather than what
        // we happened to copy.
        std::fs::set_permissions(
            from.join("credentials.enc"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();

        migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        let file_mode = std::fs::metadata(to.join("credentials.enc"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let dir_mode = std::fs::metadata(&to).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(dir_mode, 0o700);
    }

    /// The property that makes staging worth the complexity: a failure must not
    /// leave a half-populated target, because the next run would choose it and
    /// the user would appear logged out.
    #[cfg(unix)]
    #[test]
    fn a_failed_migration_leaves_no_partial_target() {
        use std::os::unix::fs::PermissionsExt;

        let home = TempDir::new().unwrap();
        let from = home.path().join(".atlassian-cli");
        seed(&from, &[("config.yaml", "x"), ("credentials.enc", "{}")]);

        let locked = home.path().join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();

        // CI often runs as root, where 0500 denies nothing.
        if std::fs::create_dir(locked.join(".probe")).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
            return;
        }

        let to = locked.join(APP_DIR);
        let (paths, outcome) = migrate_legacy_dir(ConfigPaths::for_legacy(&from, &to));

        assert!(matches!(outcome, DirMigration::Failed { .. }));
        assert_eq!(
            paths.dir(),
            from,
            "a failed migration must keep working from the old location"
        );
        assert!(from.join("config.yaml").exists(), "nothing may be lost");
        assert!(!to.exists(), "no partial target");

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    /// A directory we did not create is not ours to re-permission. Tightening it
    /// would mean `--config /shared/team.yaml` silently chmodding /shared, or
    /// `ATLASSIAN_CLI_CONFIG_DIR=$HOME` chmodding the home directory.
    #[cfg(unix)]
    #[test]
    fn an_existing_directory_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let parent = TempDir::new().unwrap();
        let dir = parent.path().join("theirs");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        create_private_dir(&dir).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o755,
            "we must not re-permission a directory we found"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_we_create_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let parent = TempDir::new().unwrap();
        let dir = parent.path().join("nested").join("ours");

        create_private_dir(&dir).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    /// The file is ours whatever the directory, because it holds the token.
    #[cfg(unix)]
    #[test]
    fn a_file_written_into_someone_elses_directory_is_still_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let parent = TempDir::new().unwrap();
        let dir = parent.path().join("theirs");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        write_private(&dir.join("config.yaml"), b"profiles: {}").unwrap();

        let mode = std::fs::metadata(dir.join("config.yaml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn nothing_happens_when_the_directory_was_chosen_explicitly() {
        let home = TempDir::new().unwrap();
        let dir = home.path().join("explicit");
        seed(&dir, &[("config.yaml", "x")]);

        let (paths, outcome) = migrate_legacy_dir(ConfigPaths::for_dir(&dir));

        assert!(matches!(outcome, DirMigration::NotNeeded));
        assert_eq!(paths.dir(), dir);
    }
}
