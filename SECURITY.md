# Security Policy

Nidus is pre-release software, but security reports are still handled as
private issues until a fix is available.

## Supported Versions

Only the current `main` branch is supported before the first published release.
After release, supported versions will be listed here.

## Reporting a Vulnerability

Do not open a public issue for suspected vulnerabilities. Report privately to
the maintainers with:

- Affected crate, feature, or generated project template.
- Steps to reproduce.
- Expected impact.
- Any relevant logs, requests, generated code, or configuration.

The project should acknowledge valid reports, assess impact, prepare a fix, and
coordinate disclosure once users have a reasonable upgrade path.

## Scope

Security-sensitive areas include generated project templates, route guards,
request validation, error responses, configuration loading, dependency graph
resolution, middleware, and CLI behavior that reads or writes project files.
