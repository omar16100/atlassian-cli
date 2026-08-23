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
        let _ = std::fs::remove_dir_all(&staging);
        if is_populated(to) {
            return Ok(copied);
        }
        return Err(anyhow!("Unable to move {} into place: {}", to.display(), e));
    }

    Ok(copied)
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
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = options
            .open(&tmp)
            .map_err(|e| anyhow!("Unable to write {}: {}", tmp.display(), e))?;
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
    std::fs::create_dir_all(dir)
        .map_err(|e| anyhow!("Unable to create directory {}: {}", dir.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir)
            .map_err(|e| anyhow!("Unable to inspect {}: {}", dir.display(), e))?
            .permissions()
            .mode();
        // Only tighten, and only when it is currently open to others.
        if mode & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| anyhow!("Unable to restrict {}: {}", dir.display(), e))?;
        }
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
