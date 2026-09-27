# Production dependency bump with argon2 0.6 (plan)

Status: merged in PR #144 (not yet in a release), supersedes Dependabot PR #143.

## Context

Dependabot PR #143 bumps the production group: clap 4.6.7, reqwest 0.13.5,
serde 1.0.229, tabled 0.22, dirs 7, futures 0.3.34, aes-gcm 0.11.1,
argon2 0.5.3 to 0.6.0 and indexmap 2.14.2. It failed Clippy and Tests because
argon2 0.6 (password-hash 0.6) removed `SaltString` and changed
`PasswordHasher::hash_password` to take the password only.

The compiler's suggested fix is the trap. The one-argument `hash_password` in
password-hash 0.6 draws a random salt, so `derive_key` would return a different
key on every call and every existing `credentials.enc` would stop decrypting.

What the old code actually hashed: argon2 0.5.3's `hash_password` base64-decodes
the `SaltString` back into bytes before calling `hash_password_into`
(`argon2-0.5.3/src/lib.rs`, `impl PasswordHasher for Argon2`). So the effective
salt was always the raw machine-id bytes, never the base64 text.

## Approach

1. Known-answer test first, on argon2 0.5.3. `derive_key` is split into
   `derive_key` (machine lookups) and `derive_key_from(machine_id, username)`
   (the derivation), and `derive_key_known_answer` pins the output for a
   synthetic UUID-shaped machine id and a made-up user name. The expected key
   was printed by the unmodified 0.5.3 code path and cross-checked against the
   reference C implementation through argon2-cffi (`hash_secret_raw`, Argon2id,
   version 19, m=19456, t=2, p=1, 32 bytes, salt = raw machine-id bytes). Using
   the base64 text as the salt gives a different key, which confirms the decode.
2. Still on 0.5.3, switch `derive_key_from` to
   `Argon2::new(Argon2id, V0x13, Params::new(19456, 2, 1, Some(32)))` and
   `hash_password_into(password, machine_id_bytes, &mut key)`. The parameters
   are named constants rather than `Argon2::default()`, so a later change of
   library default cannot move the key. The known-answer test still passes.
3. Apply Dependabot's `Cargo.toml`, `crates/auth/Cargo.toml` and `Cargo.lock`
   changes unchanged. Nothing else needed a code change: clap, tabled 0.22 and
   reqwest built cleanly, and clippy is clean.
4. dirs 7: its only breaking change is Windows `preference_dir`, which the CLI
   does not call (it uses `home_dir` and `config_local_dir`), and dirs 7 keeps
   dirs-sys 0.5. No migration. A regression test,
   `process_home_comes_from_home_and_the_default_is_dot_config`, pins what the
   resolution reads on Unix.

## Verification

- `derive_key_known_answer` passes on argon2 0.5.3 before and after the switch
  to `hash_password_into`, and on argon2 0.6.0 after the bump.
- `decrypts_real_credentials_file_copy` (ignored by default) decrypts a copy of
  a real `credentials.enc` through `CredentialStore::get_encrypted` only, so the
  token environment variables and the plaintext `credentials` fallback in
  `get_token` cannot make it pass. It prints counts, never account names or
  tokens. Run on the maintainer's macOS machine against the `credentials.enc`
  in use there (last written 26 Aug 2026, before this change): 5 of 5 entries
  decrypted on 0.5.3 and 5 of 5 on 0.6.0.
  Negative control: with one ciphertext byte flipped in the copy, the check
  fails with "1 of 5 entries failed to decrypt". Error details are never
  formatted into the output, because a serde error can quote file content.
  The copies were deleted afterwards.
- Codex review (no blocker or major): one minor, applied. The first version of
  the manual check printed the root cause of a decrypt error and the serde
  error of an unparseable file; both are now fixed messages.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` and `cargo test --all --no-fail-fast`: 911
  passed, 0 failed, 1 ignored (the manual decrypt check).

To repeat the real-file check after a future crypto bump:

```bash
dir=$(mktemp -d) && cp ~/.config/atlassian-cli/credentials.enc "$dir"/
ATLASSIAN_CLI_DECRYPT_CHECK_DIR="$dir" \
  cargo test -p atlassian-cli-auth --lib -- --ignored --nocapture decrypts_real
rm -rf "$dir"
```

## Limitations

- The real-file check ran on macOS only. Linux (`/etc/machine-id`) and Windows
  (`MachineGuid`) take the same code path with a different machine-id string;
  the known-answer test covers the derivation itself on every CI platform.
- Machine ids longer than 48 bytes used to fail in `SaltString::encode_b64` and
  now derive a key. No file can exist for such a machine, so nothing changes for
  existing users.
