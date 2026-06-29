# Security Policy

Nidus accepts private vulnerability reports for the framework crates, CLI,
generated starter template, examples, and documentation that could affect safe
use of the published crates.

## Supported Versions

| Version | Supported |
| --- | --- |
| `1.0.x` | Yes |
| `< 1.0.0` | No |

Patch releases in the `1.0.x` line are expected to receive security fixes. Users
should upgrade to the latest `1.0.x` patch when a fix is released.

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
