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
the maintainers through GitHub private vulnerability reporting:

https://github.com/vicotrbb/nidus/security/advisories/new

If GitHub private reporting is unavailable, email the maintainer at
security@rustnidus.com and include `Nidus security report` in the subject.

Include:

- Affected crate, feature, or generated project template.
- Steps to reproduce.
- Expected impact.
- Any relevant logs, requests, generated code, or configuration.

The project aims to acknowledge valid reports within 3 business days, provide an
initial assessment within 7 business days, and coordinate public disclosure only
after a fix, mitigation, or clear non-applicability assessment is available.

## Scope

Security-sensitive areas include generated project templates, route guards,
request validation, error responses, configuration loading, dependency graph
resolution, middleware, and CLI behavior that reads or writes project files.

Out of scope: denial-of-service reports that require unrealistic local resource
control, issues that depend on applications disabling Nidus security defaults,
and findings against third-party services not controlled by this repository.
