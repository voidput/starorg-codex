# Security policy

## Reporting a vulnerability

Report privately through GitHub: **Security -> Report a vulnerability** on
<https://github.com/voidput/starorg-codex/security/advisories/new>. Please do not open a public issue for
security problems.

Include the affected version, steps to reproduce, and the impact. You will get an acknowledgement as soon as
the maintainer sees it; fixes ship in the next release.

## Supported versions

Only the latest release receives security fixes.

## Scope

StarOrg Codex reads local game files and writes a patched `Data.p4k` overlay and `user.cfg` locally. Issues
such as path traversal, unsafe archive handling, or supply-chain problems in the build or release pipeline are
in scope. The CI hardening is described in [docs/ci.md](docs/ci.md).
