# Branch Protection Configuration

This document describes the branch protection rules that should be configured via GitHub's web interface for the `main` branch.

## Settings > Branches > Branch protection rules

### Main Branch (`main`)

#### Protect matching branches

**Require a pull request before merging**
- ✅ Enable
- Required approvals: 1
- ✅ Dismiss stale pull request approvals when new commits are pushed
- ✅ Require review from Code Owners

**Require status checks to pass before merging**
- ✅ Enable
- ✅ Require branches to be up to date before merging
- Required status checks:
  - `fmt` (Format Check)
  - `clippy` (Clippy Lint)
  - `test (ubuntu-latest)` (Tests - Linux)
  - `test (macos-latest)` (Tests - macOS)
  # coverage removed - was failing without token

**Require conversation resolution before merging**
- ✅ Enable

**Require linear history**
- ✅ Enable (prevents merge commits, enforces rebase/squash)

**Do not allow bypassing the above settings**
- ✅ Include administrators (ensures even repo admins follow the rules)

**Allow force pushes**
- ❌ Disable

**Allow deletions**
- ❌ Disable

## How to Configure

1. Go to: `https://github.com/omar16100/atlassian-cli/settings/branches`
2. Click "Add branch protection rule"
3. Branch name pattern: `main`
4. Apply settings as described above
5. Click "Create" or "Save changes"

## Rationale

- **PR reviews**: Prevents direct pushes, ensures code review
- **Status checks**: All CI must pass (fmt, clippy, tests)
- **Linear history**: Cleaner git log, easier to bisect
- **Include administrators**: Lead by example, no shortcuts
- **No force push**: Protects against history rewriting
- **Code Owners**: Security-sensitive changes get extra scrutiny

## Testing the Protection

After configuration:
```bash
# This should fail (direct push to main)
git push origin main

# This should work (push to feature branch, then PR)
git checkout -b feature/test
git push origin feature/test
# Then create PR via GitHub
```
