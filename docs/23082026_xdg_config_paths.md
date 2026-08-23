# XDG config paths (issue #127)

Status: shipped in 0.8.0 (PR pending).

## Problem

Everything lived in `~/.atlassian-cli/`, hardcoded in four places, and
`$XDG_CONFIG_HOME` was never read. `--config` pointed at a different config
*file*, but the two credential files had no flag and no environment variable, so
the location could not be changed at all. The workaround was a symlink.

`SECURITY.md` had meanwhile been telling users their credentials were at
`~/.config/atlassian-cli/credentials`, describing this feature as though it
already existed.

## Resolution order

1. `$ATLASSIAN_CLI_CONFIG_DIR`, or `--config-dir`
2. `$XDG_CONFIG_HOME/atlassian-cli`
3. `~/.config/atlassian-cli`, or `%LOCALAPPDATA%\atlassian-cli` on Windows
4. `~/.atlassian-cli`, then `~/.atlcli`, if either still holds our files

All three files live in whichever directory wins, so one setting moves
everything.

## Decisions worth recording

**`~/.config` on macOS.** `dirs::config_dir()` returns
`~/Library/Application Support` there. Someone asking for XDG means `~/.config`,
so `$XDG_CONFIG_HOME` is read directly and the fallback is the same on Linux and
macOS.

**`config_local_dir()` on Windows, not `config_dir()`.** The encryption key
derives from the machine id (`crates/auth/src/encryption.rs`), so a
`credentials.enc` replicated by a roaming profile cannot be decrypted on the
second machine, and would surface as a bare "Decryption failed".

**Legacy means populated, not present.** A directory counts as an install only
if it holds at least one of our three files. A leftover empty `~/.atlassian-cli`
would otherwise pin someone on the old location forever, or trigger a migration
that copies nothing and renames a directory they created on purpose.

**Relative values are treated differently.** A relative `$XDG_CONFIG_HOME` is
ignored, as the basedir spec requires: resolving it against the working
directory would drop a credentials file into whatever repository the user is
standing in. A relative `$ATLASSIAN_CLI_CONFIG_DIR` is honoured, because that
variable is ours and `./ci-config` is reasonable in a job with a fixed working
directory.

**`--config` still moves one file.** Credentials follow the directory. Making
them follow `dirname(--config)` would mean `--config ~/config.yaml` writes
`~/credentials.enc` and tightens `$HOME` to `0700`, and would break the common CI
pattern of a checked-in config plus a token from the environment.

**`auth` is told its directory.** `CredentialStore::new(dir)` replaced the free
functions that derived paths internally. That is what makes the location
movable, and it removed six `Cannot determine home directory` sites. It also
stopped the auth crate's own tests writing to the developer's real
`credentials.enc` — one of them called a function that securely deletes a real
plaintext credentials file.

## Migration

Files are copied into a staging directory and promoted with a single atomic
rename, then the original is renamed to `~/.atlassian-cli.migrated`.

Staging is not cosmetic. Writing into the target one file at a time could leave
`config.yaml` present and `credentials.enc` missing; the next run's resolver
would see a populated new directory, choose it, and the user would appear logged
out while their tokens sat in a directory the CLI no longer reads.

The archive rename is the same concern from the other end: a lingering copy that
is silently ignored is a trap, because anything edited there later has no
effect. Nothing is deleted.

`.atlcli` is a second legacy candidate rather than a chained hop, so a very old
install moves once and prints one message.

Setting `$ATLASSIAN_CLI_CONFIG_DIR` skips migration: an explicit choice is taken
at face value.

## Permissions

The directory is created `0700` and every file written `0600`, via a temporary
file and a rename. `OpenOptions::mode` applies only at creation, so writing in
place left an existing `0644` credentials file world-readable. `config.yaml` gets
the same treatment: it can hold a plaintext `api_token` and previously landed
with whatever the umask allowed.

Windows has no equivalent and relies on `%LOCALAPPDATA%` being user-only. Said
plainly in `SECURITY.md` rather than letting "0600" imply cross-platform cover.

## Not addressed

Concurrent `set_encrypted` calls remain a lost-update race: each is a
read-modify-write of the whole file. Writing through a temp file and renaming
makes each write atomic, so nobody reads a truncated file, which the previous
truncate-then-write allowed. Full locking is a separate piece of work.

## Tests

- 17 pure unit tests for resolution, over an injected environment and a
  `populated` probe, so they need no `set_var` and no lock and run in parallel.
- 10 migration tests over temporary directories, including that a forced failure
  leaves no partial target and keeps working from the old location.
- Auth tests moved onto temporary directories, plus new coverage that a file
  overwritten from `0644` ends up `0600`.
- 12 end-to-end tests in `crates/cli/tests/config_paths.rs` spawning the binary
  against a scratch `HOME`: each resolution rule, permissions on disk, migration
  including that the moved token still decrypts, a silent second run, an empty
  legacy directory left alone, an explicit directory never migrated, and
  `--config` moving only the config file.
