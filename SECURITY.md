# Security Policy

`atlassian-cli` is an independent open-source project. It is not affiliated with,
endorsed by, sponsored by, or maintained by Atlassian.

## Scope

`atlassian-cli` runs entirely on the user's own machine. API tokens are stored
locally with AES-256-GCM encryption (`~/.config/atlassian-cli/credentials`). The
project and the `atlassiancli.com` website never receive, transmit, or process
user Atlassian credentials.

## Supported versions

Security fixes are applied to the latest released version on
[crates.io](https://crates.io/crates/atlassian-cli). Please upgrade before
reporting an issue to confirm it still reproduces.

## Reporting a vulnerability

Please report suspected vulnerabilities **privately** — do not open a public
issue for security problems.

- Preferred: open a private advisory via
  [GitHub Security Advisories](https://github.com/omar16100/atlassian-cli/security/advisories/new)
- Alternative: email **omarshabab55@gmail.com** with subject `atlassian-cli security`

Please include: affected version, reproduction steps, and impact. We aim to
acknowledge within 7 days and to provide a remediation timeline after triage.

## Disclosure

Coordinated disclosure is preferred. We will credit reporters in the release
notes unless anonymity is requested.
