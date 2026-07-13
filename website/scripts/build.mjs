#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const WEBSITE = path.join(ROOT, 'website');
const SRC = path.join(WEBSITE, 'src');
const DIST = path.join(WEBSITE, 'dist');
const BASE = normalizeBase(process.env.NIDUS_SITE_BASE ?? '/');
const SITE_DOMAIN = (process.env.NIDUS_SITE_DOMAIN ?? '').trim();
const SITE_ORIGIN = SITE_DOMAIN ? `https://${SITE_DOMAIN}` : '';
const RELEASE_VERSION = '1.0.11';
const SITE_DESCRIPTION = 'Nidus is a modular Rust backend framework for explicit services, typed dependency injection, Axum routes, Tower middleware, OpenAPI, observability, testing, and installable adapters.';

const docs = [
  {
    title: 'Overview',
    slug: 'docs',
    group: 'Start',
    source: 'docs/README.md',
    summary: 'Standalone overview of the Nidus framework surface.',
  },
  {
    title: 'Installation',
    slug: 'docs/installation',
    group: 'Start',
    source: 'docs/installation.md',
    summary: 'Install the CLI, facade crate, and optional adapters.',
  },
  {
    title: 'Getting Started',
    slug: 'docs/getting-started',
    group: 'Start',
    source: 'docs/getting-started.md',
    summary: 'Create and inspect a Nidus application.',
  },
  {
    title: 'CLI',
    slug: 'docs/cli',
    group: 'Start',
    source: 'docs/cli.md',
    summary: 'Project generation and inspection commands.',
  },
  { title: 'Mental Model', slug: 'docs/mental-model', group: 'Concepts', source: 'docs/mental-model.md', summary: 'How Nidus maps modules, providers, controllers, guards, and pipes to Rust.' },
  { title: 'Architecture', slug: 'docs/architecture', group: 'Concepts', source: 'docs/architecture.md', summary: 'Workspace crates and dependency boundaries.' },
  { title: 'Modules', slug: 'docs/modules', group: 'Concepts', source: 'docs/modules.md', summary: 'Module imports, providers, controllers, exports, and graph validation.' },
  { title: 'Providers / DI', slug: 'docs/providers-di', group: 'Concepts', source: 'docs/dependency-injection.md', summary: 'Typed dependency injection, factories, optional dependencies, and request scope.' },
  { title: 'Controllers / Routes', slug: 'docs/controllers-routes', group: 'Concepts', source: 'docs/controllers.md', summary: 'Controller macros, route definitions, metadata, and Axum composition.' },
  { title: 'Guards', slug: 'docs/guards', group: 'Concepts', source: 'docs/guards.md', summary: 'Authorization guards and guard layers.' },
  { title: 'Validation / Pipes', slug: 'docs/pipes-validation', group: 'Concepts', source: 'docs/pipes.md', summary: 'Validation pipes, DTO validation, and stable 422 responses.' },
  { title: 'Interceptors / Tower Middleware', slug: 'docs/interceptors', group: 'Concepts', source: 'docs/interceptors.md', summary: 'Tower-first interception and middleware guidance.' },
  { title: 'Config', slug: 'docs/config', group: 'Runtime', source: 'docs/config.md', summary: 'Typed config from pairs, JSON, files, and environment variables.' },
  { title: 'Error Handling', slug: 'docs/error-handling', group: 'Runtime', source: 'docs/error-handling.md', summary: 'HTTP errors and production error envelopes.' },
  { title: 'OpenAPI', slug: 'docs/openapi', group: 'Runtime', source: 'docs/openapi.md', summary: 'OpenAPI route metadata and document rendering.' },
  { title: 'Observability', slug: 'docs/observability', group: 'Runtime', source: 'docs/observability.md', summary: 'Production logs, traces, metrics, events, jobs, lifecycle, and adapter instrumentation.' },
  { title: 'Dashboard', slug: 'docs/dashboard', group: 'Runtime', source: 'docs/dashboard.md', summary: 'Optional protected runtime cockpit with Home, Atlas, Timeline, storage, capture, APIs, and SSE.' },
  { title: 'Events', slug: 'docs/events', group: 'Runtime', source: 'docs/events.md', summary: 'In-process event bus and observed events.' },
  { title: 'Jobs', slug: 'docs/jobs', group: 'Runtime', source: 'docs/jobs.md', summary: 'Sync and async job queues with observed runners.' },
  { title: 'Testing', slug: 'docs/testing', group: 'Runtime', source: 'docs/testing.md', summary: 'TestApp request helpers and provider overrides.' },
  { title: 'Production Defaults', slug: 'docs/production-defaults', group: 'Production', source: 'docs/production-defaults.md', summary: 'HTTP defaults, observability defaults, and what remains explicit.' },
  { title: 'Deployment', slug: 'docs/deployment', group: 'Production', source: 'docs/deployment.md', summary: 'Deployment boundaries, logging, OTel helpers, health, and release setup.' },
  { title: 'Security Notes', slug: 'docs/security-notes', group: 'Production', source: 'docs/security-notes.md', summary: 'Security responsibilities, defaults, and limits.' },
  { title: 'Performance', slug: 'docs/performance', group: 'Production', source: 'docs/performance.md', summary: 'Benchmark surfaces and local result boundaries.' },
  { title: 'Official Adapters', slug: 'docs/official-adapters', group: 'Ecosystem', source: 'docs/official-adapters.md', summary: 'Separately installable adapter model and dependency boundaries.' },
  { title: 'SQLx', slug: 'docs/sqlx', group: 'Ecosystem', source: 'docs/sqlx.md', summary: 'SQLx adapter features, pool registration, health, and observability.' },
  { title: 'Cache', slug: 'docs/cache', group: 'Ecosystem', source: 'docs/cache.md', summary: 'Moka cache adapter features, health, and observability.' },
  { title: 'Integration Contract', slug: 'docs/integrations', group: 'Ecosystem', source: 'docs/integrations.md', summary: 'Adapter contract, backend feature flags, and current limitations.' },
  { title: 'Examples', slug: 'docs/examples', group: 'Ecosystem', source: 'docs/examples.md', summary: 'Workspace examples and validation commands.' },
  {
    title: 'API Reference',
    slug: 'docs/api-reference',
    group: 'Reference',
    source: 'docs/api-reference.md',
    summary: 'Crate map and generated Rust API reference entry points.',
  },
  {
    title: 'Release 1.0.11',
    slug: 'docs/release-1-0-11',
    group: 'Reference',
    source: 'docs/release-1-0-11.md',
    summary: 'Nidus 1.0.11 OpenAPI allocation improvements and lifecycle shutdown hardening.',
  },
  {
    title: 'Release 1.0.10',
    slug: 'docs/release-1-0-10',
    group: 'Reference',
    source: 'docs/release-1-0-10.md',
    summary: 'Nidus 1.0.10 first-party integration ecosystem and delivery guarantees.',
  },
  {
    title: 'Release 1.0.9',
    slug: 'docs/release-1-0-9',
    group: 'Reference',
    source: 'docs/release-1-0-9.md',
    summary: 'Nidus 1.0.9 routing and OpenAPI builder performance notes.',
  },
  {
    title: 'Release 1.0.8',
    slug: 'docs/release-1-0-8',
    group: 'Reference',
    source: 'docs/release-1-0-8.md',
    summary: 'Nidus 1.0.8 controller, request-context, and OpenAPI performance notes.',
  },
  {
    title: 'Release 1.0.7',
    slug: 'docs/release-1-0-7',
    group: 'Reference',
    source: 'docs/release-1-0-7.md',
    summary: 'Nidus 1.0.7 performance, trace correctness, and runtime hardening notes.',
  },
  {
    title: 'Release 1.0.6',
    slug: 'docs/release-1-0-6',
    group: 'Reference',
    source: 'docs/release-1-0-6.md',
    summary: 'Nidus 1.0.6 release notes and performance evidence.',
  },
  {
    title: 'Release 1.0.5',
    slug: 'docs/release-1-0-5',
    group: 'Reference',
    source: 'docs/release-1-0-5.md',
    summary: 'Nidus 1.0.5 release notes and proof boundaries.',
  },
  {
    title: 'Release 1.0.4',
    slug: 'docs/release-1-0-4',
    group: 'Reference',
    source: 'docs/release-1-0-4.md',
    summary: 'Nidus 1.0.4 release notes and proof boundaries.',
  },
  {
    title: 'Release 1.0.3',
    slug: 'docs/release-1-0-3',
    group: 'Reference',
    source: 'docs/release-1-0-3.md',
    summary: 'Nidus 1.0.3 release notes and proof boundaries.',
  },
];

const docSlugBySource = new Map(
  docs
    .filter((doc) => doc.source)
    .map((doc) => [doc.source.replace(/^docs\//, '').replace(/\.md$/, ''), doc.slug]),
);

function normalizeBase(base) {
  if (!base || base === '/') return '/';
  return `/${base.replace(/^\/+|\/+$/g, '')}/`;
}

function href(slug = '') {
  return `${BASE}${slug}`.replace(/\/+/g, '/');
}

function asset(name) {
  return href(`assets/${name}`);
}

function absoluteHref(slug = '') {
  const path = href(slug);
  return SITE_ORIGIN ? `${SITE_ORIGIN}${path}` : path;
}

function read(file) {
  return fs.readFileSync(path.join(ROOT, file), 'utf8');
}

function escapeHtml(value) {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function escapeJsonForHtml(value) {
  return JSON.stringify(value).replace(/</g, '\\u003c');
}

function seoDescription(description) {
  if (description.length >= 50) return description;
  return `${description} Nidus docs for typed Rust services, explicit modules, Axum routing, and production backend composition.`;
}

function documentTitle(title) {
  if (title === 'Nidus') return 'Nidus Rust backend framework';
  if (title.length < 8) return `Nidus ${title} documentation`;
  return `${title} · Nidus`;
}

function structuredData({ title, description, url, currentSlug, home }) {
  const graph = [
    {
      '@type': 'WebSite',
      '@id': `${absoluteHref()}#website`,
      url: absoluteHref(),
      name: 'Nidus',
      description: SITE_DESCRIPTION,
      inLanguage: 'en',
    },
    {
      '@type': home ? 'SoftwareSourceCode' : 'TechArticle',
      '@id': `${url}#content`,
      name: title,
      headline: title,
      description,
      url,
      image: absoluteHref('assets/og-image.png'),
      inLanguage: 'en',
      isPartOf: { '@id': `${absoluteHref()}#website` },
      publisher: {
        '@type': 'Organization',
        name: 'Nidus',
        url: absoluteHref(),
        logo: {
          '@type': 'ImageObject',
          url: absoluteHref('assets/logo-mark-transparent.png'),
        },
      },
      programmingLanguage: 'Rust',
      codeRepository: 'https://github.com/vicotrbb/nidus',
    },
  ];

  if (!home && currentSlug) {
    graph.push({
      '@type': 'BreadcrumbList',
      '@id': `${url}#breadcrumb`,
      itemListElement: [
        { '@type': 'ListItem', position: 1, name: 'Nidus', item: absoluteHref() },
        { '@type': 'ListItem', position: 2, name: 'Docs', item: absoluteHref('docs/') },
        { '@type': 'ListItem', position: 3, name: title, item: url },
      ],
    });
  }

  return { '@context': 'https://schema.org', '@graph': graph };
}

function inline(md) {
  return escapeHtml(md)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_, text, url) => {
      const target = url.startsWith('http') ? url : href(resolveDocLink(url));
      return `<a href="${target}">${text}</a>`;
    });
}

function resolveDocLink(url) {
  if (!url.endsWith('.md')) return url;
  const key = url.replace(/^\.?\//, '').replace(/^docs\//, '').replace(/\.md$/, '');
  return docSlugBySource.get(key) ?? `docs/${key}`;
}

function slugify(text) {
  return text.toLowerCase().replace(/`/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

function markdownToHtml(markdown) {
  const lines = markdown.replace(/\r\n/g, '\n').split('\n');
  let html = '';
  let paragraph = [];
  let list = null;
  let code = null;
  let table = [];

  const flushParagraph = () => {
    if (paragraph.length) {
      html += `<p>${inline(paragraph.join(' '))}</p>\n`;
      paragraph = [];
    }
  };
  const flushList = () => {
    if (list) {
      html += `</${list}>\n`;
      list = null;
    }
  };
  const flushTable = () => {
    if (!table.length) return;
    const rows = table.map((row) => row.split('|').slice(1, -1).map((cell) => inline(cell.trim())));
    const [head, separator, ...body] = rows;
    if (separator && separator.every((cell) => /^:?-+:?$/.test(cell.replace(/<[^>]*>/g, '')))) {
      html += '<table><thead><tr>';
      html += head.map((cell) => `<th>${cell}</th>`).join('');
      html += '</tr></thead><tbody>';
      for (const row of body) html += `<tr>${row.map((cell) => `<td>${cell}</td>`).join('')}</tr>`;
      html += '</tbody></table>\n';
    } else {
      html += table.map((row) => `<p>${inline(row)}</p>`).join('\n');
    }
    table = [];
  };

  for (const line of lines) {
    if (code) {
      if (line.startsWith('```')) {
        html += `<pre><code class="language-${code.lang}">${escapeHtml(code.lines.join('\n'))}</code></pre>\n`;
        code = null;
      } else {
        code.lines.push(line);
      }
      continue;
    }
    if (line.startsWith('```')) {
      flushParagraph();
      flushList();
      flushTable();
      code = { lang: escapeHtml(line.slice(3).trim()), lines: [] };
      continue;
    }
    if (/^\|.*\|$/.test(line.trim())) {
      flushParagraph();
      flushList();
      table.push(line.trim());
      continue;
    }
    flushTable();
    if (!line.trim()) {
      flushParagraph();
      flushList();
      continue;
    }
    const heading = /^(#{1,4})\s+(.+)$/.exec(line);
    if (heading) {
      flushParagraph();
      flushList();
      const level = heading[1].length;
      const text = heading[2].trim();
      html += `<h${level} id="${slugify(text)}">${inline(text)}</h${level}>\n`;
      continue;
    }
    const bullet = /^[-*]\s+(.+)$/.exec(line);
    if (bullet) {
      flushParagraph();
      if (list !== 'ul') {
        flushList();
        list = 'ul';
        html += '<ul>\n';
      }
      html += `<li>${inline(bullet[1])}</li>\n`;
      continue;
    }
    const number = /^\d+\.\s+(.+)$/.exec(line);
    if (number) {
      flushParagraph();
      if (list !== 'ol') {
        flushList();
        list = 'ol';
        html += '<ol>\n';
      }
      html += `<li>${inline(number[1])}</li>\n`;
      continue;
    }
    paragraph.push(line.trim());
  }
  flushParagraph();
  flushList();
  flushTable();
  return html;
}

function extractToc(markdown) {
  return markdown
    .replace(/\r\n/g, '\n')
    .split('\n')
    .map((line) => /^(#{2,3})\s+(.+)$/.exec(line))
    .filter(Boolean)
    .slice(0, 8)
    .map((match) => ({
      level: match[1].length,
      title: match[2].replace(/`/g, '').trim(),
      id: slugify(match[2]),
    }));
}

function apiReference() {
  const crates = [
    ['nidus-rs', 'nidus', 'Facade crate and prelude'],
    ['nidus-core', 'nidus_core', 'Modules, DI, lifecycle, and app bootstrap'],
    ['nidus-http', 'nidus_http', 'Controllers, routing, middleware, health, metrics, logging, OTel helpers'],
    ['nidus-macros', 'nidus_macros', 'Controller, route, module, provider, guard, pipe, and entrypoint macros'],
    ['nidus-config', 'nidus_config', 'Typed configuration values and loaders'],
    ['nidus-openapi', 'nidus_openapi', 'OpenAPI route metadata and document generation'],
    ['nidus-validation', 'nidus_validation', 'Validation pipes and JSON extractors backed by garde'],
    ['nidus-auth', 'nidus_auth', 'Guard traits, combinators, and Tower layers'],
    ['nidus-events', 'nidus_events', 'Event bus and observed event dispatch'],
    ['nidus-jobs', 'nidus_jobs', 'Job queues and observed job runners'],
    ['nidus-testing', 'nidus_testing', 'TestApp request harness and provider overrides'],
    ['nidus-sqlx', 'nidus_sqlx', 'Official SQLx adapter'],
    ['nidus-cache', 'nidus_cache', 'Official Moka cache adapter'],
    ['cargo-nidus', '', 'CLI generator and source inspector'],
  ];
  const rows = crates.map(([packageName, crateName, summary]) => {
    const reference = crateName
      ? `https://docs.rs/${packageName}/${RELEASE_VERSION}/${crateName}/`
      : `https://docs.rs/${packageName}/${RELEASE_VERSION}/`;
    return `| \`${packageName}\` | ${summary} | ${reference} |`;
  }).join('\n');
  return `# API Reference

The release website links to generated Rust API references on docs.rs once the crates are published. During local launch verification, build the same reference set with:

\`\`\`bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
\`\`\`

After publishing, verify the docs.rs pages with:

\`\`\`bash
bash scripts/verify-published-release.sh ${RELEASE_VERSION}
\`\`\`

| Crate | Surface | Reference |
| --- | --- | --- |
${rows}

The facade crate keeps core Nidus ergonomic, while SQLx and cache integrations remain separate installable crates.`;
}

function releaseNotes() {
  return `# Release ${RELEASE_VERSION}

Nidus ${RELEASE_VERSION} is the public website, documentation, and launch-surface release for the framework surface in this repository.

## Highlights

- Standalone modular Rust application structure with modules, controllers, providers, guards, validation, config, OpenAPI, events, jobs, testing, and production HTTP defaults.
- Lean \`nidus-rs\` facade package, imported as \`nidus\` in Rust code, with SQLx and cache integrations delivered as separately installable official adapters.
- \`cargo-nidus\` project generation plus source inspection commands for routes, module graphs, macro expansion, checks, and OpenAPI.
- Custom-domain website output for \`rustnidus.com\`, including root-base asset paths, generated \`CNAME\`, link checks, docs search, and a generated 404 page.
- Expanded public docs for installation, CLI, concepts, runtime surfaces, production boundaries, official adapters, examples, API reference, and release proof.

## Proof Boundary

Local verification can prove package dry-runs, tests, docs, website output, link checks, visual rendering, and runtime examples. Actual crates.io publishing, docs.rs rendering, GitHub Pages deployment settings, and DNS state require credentials or external repository settings, so those steps must be reported separately when blocked.

After publishing the crates, verify the public package and documentation state:

\`\`\`bash
bash scripts/verify-published-release.sh ${RELEASE_VERSION}
\`\`\`

That command checks every publishable crate on crates.io, waits for each docs.rs package page, and then runs the external examples against their checked-in crates.io dependencies.`;
}

function loadDoc(doc) {
  return doc.markdown ?? read(doc.source);
}

function docsNav(currentSlug) {
  const groups = [];
  for (const doc of docs) {
    let group = groups.find((entry) => entry.name === doc.group);
    if (!group) {
      group = { name: doc.group, docs: [] };
      groups.push(group);
    }
    group.docs.push(doc);
  }

  return groups.map((group) => `<section class="docs-nav-group">
    <h2>${escapeHtml(group.name)}</h2>
    ${group.docs.map((doc) => `<a class="docs-link" href="${href(`${doc.slug}/`)}" data-title="${escapeHtml(doc.title.toLowerCase())}" data-summary="${escapeHtml(doc.summary.toLowerCase())}" ${doc.slug === currentSlug ? 'aria-current="page"' : ''}>
      <span>${escapeHtml(doc.title)}</span>
      <small>${escapeHtml(doc.summary)}</small>
    </a>`).join('')}
  </section>`).join('');
}

function docsPager(currentSlug) {
  const index = docs.findIndex((doc) => doc.slug === currentSlug);
  const prev = docs[index - 1];
  const next = docs[index + 1];
  return `<nav class="docs-pager" aria-label="Documentation pagination">
    ${prev ? `<a href="${href(`${prev.slug}/`)}"><span>Previous</span><strong>${escapeHtml(prev.title)}</strong></a>` : '<span></span>'}
    ${next ? `<a href="${href(`${next.slug}/`)}"><span>Next</span><strong>${escapeHtml(next.title)}</strong></a>` : '<span></span>'}
  </nav>`;
}

function installationDoc() {
  return `# Installation

Install the Nidus CLI from crates.io:

\`\`\`bash
cargo install cargo-nidus --version ${RELEASE_VERSION}
cargo nidus new hello-nidus
cd hello-nidus
cargo run
\`\`\`

During local framework development, install directly from this checkout:

\`\`\`bash
cargo install --path crates/cargo-nidus
cargo nidus new hello-nidus
\`\`\`

Applications depend on the facade crate and opt into feature groups explicitly:

\`\`\`toml
[dependencies]
nidus = { package = "nidus-rs", version = "${RELEASE_VERSION}", features = ["http", "config", "openapi", "validation"] }
\`\`\`

Official adapters are separate crates, so the core facade stays lean:

\`\`\`toml
nidus-sqlx = { version = "${RELEASE_VERSION}", features = ["sqlite"] }
nidus-cache = { version = "${RELEASE_VERSION}", features = ["moka"] }
\`\`\`

## Feature Flags

Enable only the surfaces your application owns:

| Feature | Use when |
| --- | --- |
| \`http\` | composing Axum routers, controllers, middleware, health, metrics, and server defaults |
| \`config\` | loading typed app settings from JSON, files, pairs, or environment values |
| \`openapi\` | collecting route metadata and rendering OpenAPI JSON |
| \`validation\` | validating DTOs through garde-backed pipes and extractors |
| \`auth\` | defining guard traits, guard combinators, or Tower guard layers |
| \`events\` | dispatching in-process application events |
| \`jobs\` | running sync or async job queues |
| \`observability\` | wiring logs, metrics, traces, lifecycle validation, and adapter instrumentation |
| \`otel\` | enabling OpenTelemetry trace-context helpers through the HTTP surface |

## Imports

Use the prelude in application entrypoints:

\`\`\`rust
use nidus::prelude::*;
\`\`\`

The prelude keeps extension traits such as \`ApplicationHttpExt\`, \`NidusApplicationExt\`, and \`ApiDefaultsObservabilityExt\` in scope. If a fluent method is missing, check the feature flag and import first.

## Ownership Boundary

Nidus owns the framework composition points: module metadata, provider registration, controller metadata, guard and pipe hooks, OpenAPI route metadata, production HTTP defaults, observed events, observed jobs, and official adapter builders.

Axum, Tower, Tokio, serde, garde, utoipa, SQLx, Moka, and tracing remain normal Rust ecosystem tools. Raw SQL queries, cache-client behavior, business authorization policy, persistence migrations, deployment manifests, and external queues stay application-owned unless the app chooses an adapter or middleware boundary.`;
}

function cliDoc() {
  return `# CLI

\`cargo-nidus\` provides project generation and source inspection commands:

\`\`\`bash
cargo nidus new hello-nidus
cargo nidus check
cargo nidus routes
cargo nidus graph
cargo nidus openapi
cargo nidus expand --dry-run
\`\`\`

| Command | Purpose | Expected output |
| --- | --- | --- |
| \`cargo nidus new <name>\` | create a starter service | a Cargo project with \`src/main.rs\`, one module, one controller, and one injected service |
| \`cargo nidus check\` | validate generated project structure | success when crate roots, generated modules, and feature directories are consistent |
| \`cargo nidus routes\` | inspect controller route metadata | HTTP methods, normalized paths, summaries, guards, pipes, and validation markers when present |
| \`cargo nidus graph\` | inspect module metadata | root and feature modules plus imports, providers, controllers, and exports |
| \`cargo nidus openapi\` | render OpenAPI JSON from route metadata | a JSON document with configurable title and version |
| \`cargo nidus expand --dry-run\` | show the macro expansion command | the \`cargo expand\` invocation without running it |

The CLI is source-driven. It inspects Rust files and macro metadata rather than depending on hidden runtime registration. Use it before commits when route shape, module graph shape, or OpenAPI output matters.`;
}

function productionDefaultsDoc() {
  return `# Production Defaults

Nidus production defaults are opt-in composition helpers over Axum and Tower. They return normal routers and layers so applications can inspect, replace, or reorder the boundary.

\`\`\`rust
use nidus::prelude::*;

let app = Nidus::create::<AppModule>()
    .build()
    .await?
    .map_router(|router| {
        ApiDefaults::production("orders-api")
            .without_metrics()
            .apply(router)
    });
\`\`\`

## Included HTTP Defaults

- request IDs and request context
- health and readiness routes
- Prometheus-style metrics route when enabled
- CORS, body limits, timeout responses, security headers, and structured logging
- production error envelopes
- OpenTelemetry trace-context helpers when the \`otel\` feature is enabled

## Observability Defaults

\`\`\`rust
let observability = Observability::production("orders-api")
    .version(env!("CARGO_PKG_VERSION"))
    .environment("prod")
    .prometheus()
    .tracing()
    .otel_from_env();
\`\`\`

Automatic instrumentation applies where Nidus owns the integration point: HTTP middleware, \`ObservedEventBus\`, \`ObservedJobRunner\`, module validation, and official adapter builders. Raw SQLx queries, raw cache clients, ORMs, queues, and HTTP clients remain explicit application instrumentation.`;
}

function securityNotesDoc() {
  return `# Security Notes

Nidus provides framework boundaries that help keep service behavior inspectable, but it does not replace application security design.

## Provided Boundaries

- guard traits, guard combinators, and Tower guard layers for authorization boundaries
- typed validation pipes and stable validation error responses
- production HTTP defaults for security headers, body limits, timeouts, request IDs, and error envelopes
- explicit feature flags so optional surfaces and dependencies remain visible in Cargo manifests
- source-driven CLI inspection for routes and module graphs

## Application Responsibilities

- authentication protocol selection, key management, session policy, and credential storage
- authorization rules and tenant isolation semantics
- SQL migrations, query review, transaction boundaries, and data-retention policy
- cache key design and cache invalidation semantics
- deployment TLS, DNS, secrets, network policy, and runtime sandboxing
- security review of any raw Axum/Tower layers added outside the Nidus defaults

## Release Boundary

Local verification can prove tests, docs, package dry-runs, and example runtime behavior. crates.io publication, docs.rs rendering, GitHub Pages settings, and DNS state are external systems and must be verified after release.`;
}

function officialAdaptersDoc() {
  return `# Official Adapters

Official adapters are separately installable crates. The facade stays lean, and vendor dependencies enter the application only when the application chooses that backend.

\`\`\`toml
nidus = { package = "nidus-rs", version = "${RELEASE_VERSION}", features = ["http", "config"] }
nidus-sqlx = { version = "${RELEASE_VERSION}", features = ["sqlite", "health", "observability"] }
nidus-cache = { version = "${RELEASE_VERSION}", features = ["moka", "health", "observability"] }
\`\`\`

Adapters should register typed providers, expose health/readiness hooks when useful, add observability at adapter-owned boundaries, and still leave direct access to the underlying ecosystem client.`;
}

function sqlxDoc() {
  return `# SQLx

\`nidus-sqlx\` provides official SQLx adapter primitives for pool registration, optional config loading, health checks, and observability hooks.

\`\`\`toml
nidus-sqlx = { version = "${RELEASE_VERSION}", features = ["sqlite", "nidus-config", "health", "observability"] }
\`\`\`

Use \`sqlite\` or \`postgres\` to select the SQLx backend. Add \`nidus-config\` when pool settings should come from Nidus config, \`health\` when readiness should validate database connectivity, and \`observability\` when adapter-owned operations should emit framework observability.

Nidus does not own your schema migrations, query design, ORM layer, or transaction policy. Those stay in SQLx and application code.`;
}

function cacheDoc() {
  return `# Cache

\`nidus-cache\` provides official cache adapter primitives, including Moka-backed cache modules.

\`\`\`toml
nidus-cache = { version = "${RELEASE_VERSION}", features = ["moka", "health", "observability"] }
\`\`\`

Use \`moka\` for the default async cache backend. Add \`health\` when readiness should expose cache-state checks, and \`observability\` when adapter-owned operations should emit framework observability.

Nidus does not decide cache keys, TTL policy, invalidation semantics, or data consistency guarantees. Those remain application architecture decisions.`;
}

const benchmarkProfiles = [
  ['ping', 'rust-nidus', '156.890883/s', '423.72us', '659.57us'],
  ['ping', 'python-fastapi', '148.264889/s', '3.35ms', '4.22ms'],
  ['ping', 'java-spring', '154.820432/s', '1.13ms', '1.88ms'],
  ['ping', 'node-express', '155.442129/s', '1.08ms', '1.78ms'],
  ['users', 'rust-nidus', '297.482161/s', '1.59ms', '3.45ms'],
  ['users', 'python-fastapi', '278.858057/s', '3.32ms', '6.01ms'],
  ['users', 'java-spring', '292.89576/s', '1.97ms', '3.82ms'],
  ['users', 'node-express', '291.549083/s', '2.11ms', '4.31ms'],
  ['projects', 'rust-nidus', '423.939646/s', '1.95ms', '3.46ms'],
  ['projects', 'python-fastapi', '380.644823/s', '4.03ms', '7.17ms'],
  ['projects', 'java-spring', '417.118223/s', '2.26ms', '4.17ms'],
  ['projects', 'node-express', '414.234335/s', '2.37ms', '4.24ms'],
  ['events', 'rust-nidus', '282.357362/s', '2.97ms', '4.07ms'],
  ['events', 'python-fastapi', '267.55211/s', '4.5ms', '6.86ms'],
  ['events', 'java-spring', '281.790144/s', '3.03ms', '4.22ms'],
  ['events', 'node-express', '279.668233/s', '3.23ms', '4.65ms'],
  ['mixed', 'rust-nidus', '232.594368/s', '2.72ms', '7.05ms'],
  ['mixed', 'python-fastapi', '223.820298/s', '4.01ms', '8.35ms'],
  ['mixed', 'java-spring', '232.940412/s', '2.7ms', '6.84ms'],
  ['mixed', 'node-express', '230.302808/s', '3.03ms', '7.3ms'],
];

const benchmarkHighlights = [
  ['Fastest ping latency', '423.72us', 'Rust/Nidus average on the read-only ping profile.'],
  ['Zero failed requests', '0.00% failed', 'Every stack completed the k6 profiles without HTTP failures.'],
  ['Best write-heavy profile', '423.94/s', 'Rust/Nidus on the projects flow under the paced workload.'],
];

function benchmarkRows() {
  return benchmarkProfiles.map(([profile, stack, throughput, average, p95]) => `<tr>
    <td>${profile}</td>
    <td><code>${stack}</code></td>
    <td>${throughput}</td>
    <td>${average}</td>
    <td>${p95}</td>
    <td>0.00%</td>
    <td>100.00%</td>
  </tr>`).join('');
}

function pageShell({ title, description, body, currentSlug, home = false, standalone = false, toc = [], noindex = false }) {
  const metaTitle = documentTitle(title);
  const metaDescription = seoDescription(description);
  const canonicalPath = home || !currentSlug ? '' : `${currentSlug}/`;
  const canonicalUrl = absoluteHref(canonicalPath);
  const ogImage = absoluteHref('assets/og-image.png');
  const jsonLd = structuredData({
    title: metaTitle,
    description: metaDescription,
    url: canonicalUrl,
    currentSlug,
    home,
  });
  const tocLinks = toc.length
    ? toc.map((item) => `<a class="toc-level-${item.level}" href="#${item.id}">${escapeHtml(item.title)}</a>`).join('')
    : '<span>No section headings</span>';
  const footerColumns = [
    {
      title: 'Learn',
      links: [
        ['Docs', href('docs/')],
        ['Install', href('docs/installation/')],
        ['Examples', href('docs/examples/')],
        ['API reference', href('docs/api-reference/')],
      ],
    },
    {
      title: 'Framework',
      links: [
        ['Modules', href('docs/modules/')],
        ['Production defaults', href('docs/production-defaults/')],
        ['Official adapters', href('docs/official-adapters/')],
        ['Release notes', href('docs/release-1-0-5/')],
      ],
    },
    {
      title: 'Project',
      links: [
        ['GitHub', 'https://github.com/vicotrbb/nidus'],
        ['crates.io', 'https://crates.io/crates/nidus-rs'],
        ['docs.rs', `https://docs.rs/nidus-rs/${RELEASE_VERSION}/nidus/`],
        ['Benchmarks', href('benchmarks/')],
        ['Security', href('docs/security-notes/')],
      ],
    },
  ];

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(metaTitle)}</title>
  <meta name="application-name" content="Nidus">
  <meta name="author" content="Nidus">
  <meta name="description" content="${escapeHtml(metaDescription)}">
  ${noindex ? '<meta name="robots" content="noindex, follow">' : '<meta name="robots" content="index, follow">'}
  <link rel="canonical" href="${canonicalUrl}">
  <link rel="alternate" hreflang="en" href="${canonicalUrl}">
  <link rel="alternate" hreflang="x-default" href="${canonicalUrl}">
  <meta property="og:type" content="website">
  <meta property="og:site_name" content="Nidus">
  <meta property="og:locale" content="en_US">
  <meta property="og:url" content="${canonicalUrl}">
  <meta property="og:title" content="${escapeHtml(metaTitle)}">
  <meta property="og:description" content="${escapeHtml(metaDescription)}">
  <meta property="og:image" content="${ogImage}">
  <meta property="og:image:secure_url" content="${ogImage}">
  <meta property="og:image:type" content="image/png">
  <meta property="og:image:width" content="1200">
  <meta property="og:image:height" content="630">
  <meta property="og:image:alt" content="Nidus Rust backend framework">
  <meta name="twitter:card" content="summary_large_image">
  <meta name="twitter:title" content="${escapeHtml(metaTitle)}">
  <meta name="twitter:description" content="${escapeHtml(metaDescription)}">
  <meta name="twitter:image" content="${ogImage}">
  <meta name="twitter:image:alt" content="Nidus Rust backend framework">
  <link rel="icon" href="${asset('favicon-32.png')}" sizes="32x32">
  <link rel="apple-touch-icon" href="${asset('apple-touch-icon.png')}">
  <link rel="stylesheet" href="${href('styles.css')}">
  <script type="application/ld+json">${escapeJsonForHtml(jsonLd)}</script>
</head>
<body class="${home ? 'home' : 'doc-page'}">
  <header class="site-header">
    <a class="brand" href="${href()}" aria-label="Nidus home">
      <img src="${asset('logo-mark-transparent.png')}" alt="" width="48" height="46">
      <span>Nidus</span>
    </a>
    <button class="nav-toggle" type="button" aria-expanded="false" aria-controls="site-nav">Menu</button>
    <nav id="site-nav" class="site-nav" aria-label="Primary">
      <a href="${href('docs/')}">Docs</a>
      <a href="${href('docs/installation/')}">Install</a>
      <a href="${href('docs/examples/')}">Examples</a>
      <a href="${href('benchmarks/')}">Benchmarks</a>
      <a href="${href('docs/api-reference/')}">API</a>
      <a href="https://github.com/vicotrbb/nidus">Source</a>
    </nav>
  </header>
  ${home || standalone ? body : `<main class="docs-frame">
    <aside class="docs-sidebar">
      <div class="docs-search">
        <label for="docs-filter">Search docs</label>
        <input id="docs-filter" type="search" placeholder="modules, guards, sqlx">
      </div>
      <nav class="docs-links" aria-label="Documentation">
        ${docsNav(currentSlug)}
        <p class="docs-empty" hidden>No matching docs.</p>
      </nav>
    </aside>
    <div class="docs-page-shell">
      <article class="doc-content">${body}</article>
      <aside class="docs-toc" aria-label="On this page">
        <h2>On this page</h2>
        ${tocLinks}
      </aside>
    </div>
  </main>`}
  <footer class="site-footer">
    <div class="footer-inner">
      <div class="footer-top">
        <div class="footer-brand">
          <a class="footer-brand-link" href="${href()}" aria-label="Nidus home">
            <img src="${asset('logo-mark-transparent.png')}" alt="" width="40" height="40">
            <span>Nidus</span>
          </a>
          <p>A modular Rust backend framework for explicit services, inspectable modules, typed dependency injection, and separately installable adapters.</p>
        </div>
        <div class="footer-columns">
          ${footerColumns.map((column) => `<nav class="footer-column" aria-label="${column.title}">
            <h2>${column.title}</h2>
            ${column.links.map(([label, url]) => `<a href="${url}">${label}</a>`).join('')}
          </nav>`).join('')}
        </div>
      </div>
      <div class="footer-bottom">
        <span>Nidus ${RELEASE_VERSION}</span>
        <span>Apache-2.0 OR MIT</span>
        <a href="https://github.com/vicotrbb/nidus">Source</a>
      </div>
    </div>
  </footer>
  <script src="${href('app.js')}" type="module"></script>
</body>
</html>`;
}

function homePage() {
  const starterCommand = `cargo install cargo-nidus --version ${RELEASE_VERSION}
cargo nidus new hello-nidus
cd hello-nidus
cargo run`;
  const starterCommandAttr = escapeHtml(starterCommand).replaceAll('\n', '&#10;');
  const trustChips = ['Typed DI', 'Axum routes', 'OpenAPI', 'Production defaults', `Release ${RELEASE_VERSION}`];
  const surfaces = [
    ['Modules', 'docs/modules', 'Explicit imports, providers, controllers, exports, and graph validation.'],
    ['Controllers', 'docs/controllers-routes', 'Axum-backed route composition with Nidus metadata where it matters.'],
    ['Dependency injection', 'docs/providers-di', 'Typed providers, factories, request scope, optional dependencies, and overrides.'],
    ['Guards', 'docs/guards', 'Authorization boundaries as Rust types instead of hidden decorators.'],
    ['Validation', 'docs/pipes-validation', 'garde-backed DTO validation and stable error responses.'],
    ['OpenAPI', 'docs/openapi', 'Inspectable route metadata and generated documents from source.'],
  ];

  const proof = [
    ['Install path', 'CLI install, facade dependency, and adapter crates are separated in docs.'],
    ['Runtime defaults', 'Request IDs, context, health, metrics, CORS, limits, timeouts, security headers, tracing.'],
    ['Examples', 'launchpad-api, realworld-api, and dashboard-api exercise modules, validation, OpenAPI, health, metrics, events, jobs, and the runtime cockpit.'],
    ['Release boundary', 'Local dry-runs prove packageability; crates.io, docs.rs, and Pages deployment stay explicit external steps.'],
  ];

  const docsFor60Seconds = [
    ['Install', 'docs/installation', 'Get the CLI and facade dependency shape.'],
    ['Mental model', 'docs/mental-model', 'See what happens at build time, startup, and per request.'],
    ['Examples', 'docs/examples', 'Jump to runnable services, including launchpad-api.'],
    ['Dashboard', 'docs/dashboard', 'Inspect the optional runtime cockpit and API surface.'],
    ['Production', 'docs/production-defaults', 'Inspect HTTP defaults and deployment boundaries.'],
  ];
  const examplePaths = [
    ['Start small', 'hello-world and launchpad-api show the first controller, module, and server loop.'],
    ['Build real services', 'realworld-api and production-api cover validation, health, metrics, guards, limits, events, and jobs.'],
    ['Add adapters', 'sqlx-app, cache-app, and integrations-production show optional official crates without bloating core.'],
    ['Copy from outside', 'external-support-desk and external-commerce use crates.io-style manifests for app-shaped examples.'],
  ];

  const body = `<main>
    <section class="hero">
      <div class="hero-copy">
        <p class="eyebrow">Rust backend framework · ${RELEASE_VERSION}</p>
        <h1>Nidus</h1>
        <img class="mobile-hero-mark" src="${asset('logo-full-transparent.png')}" alt="Nidus logo" width="280" height="298">
        <p class="hero-text">A modular Rust backend framework for explicit services: typed dependency injection, module graphs, Axum routes, Tower middleware, validation, OpenAPI, observability, testing, and installable adapters.</p>
        <div class="hero-actions">
          <a class="button primary" href="${href('docs/installation/')}">Get started</a>
          <a class="button secondary" href="${href('docs/')}">Docs</a>
          <a class="button ghost" href="${href('docs/examples/')}">Examples</a>
          <a class="button ghost" href="https://github.com/vicotrbb/nidus">GitHub</a>
        </div>
        <ul class="trust-row" aria-label="Framework properties">
          ${trustChips.map((chip) => `<li>${chip}</li>`).join('')}
        </ul>
      </div>
      <div class="hero-proof" aria-label="Install and code sample">
        <img src="${asset('logo-full-transparent.png')}" alt="Nidus logo" width="689" height="658">
        <div class="install-command">
          <div>
            <span>Starter flow</span>
            <pre><code>${escapeHtml(starterCommand)}</code></pre>
          </div>
          <button type="button" data-copy="${starterCommandAttr}">Copy</button>
        </div>
        <pre class="code-panel"><code>use nidus::prelude::*;

#[controller("/users")]
struct UsersController {
    service: Inject&lt;UsersService&gt;,
}

#[module(
    providers(UsersService),
    controllers(UsersController)
)]
struct AppModule;</code></pre>
      </div>
    </section>

    <section class="first-minute" aria-labelledby="first-minute-title">
      <div>
        <p class="eyebrow">Evaluation path</p>
        <h2 id="first-minute-title">A senior Rust engineer should know where to start in under a minute.</h2>
      </div>
      <div class="minute-links">
        ${docsFor60Seconds.map(([title, slug, text], index) => `<a href="${href(`${slug}/`)}">
          <span>0${index + 1}</span>
          <strong>${title}</strong>
          <small>${text}</small>
        </a>`).join('')}
      </div>
    </section>

    <section class="concept-model" aria-labelledby="model-title">
      <div class="section-heading">
        <p class="eyebrow">Core model</p>
        <h2 id="model-title">Framework structure without hidden runtime magic.</h2>
      </div>
      <div class="model-flow">
        <article>
          <span>01</span>
          <h3>Module graph</h3>
          <p>Declare imports, providers, controllers, and exports in Rust. The graph stays visible to source inspection.</p>
        </article>
        <article>
          <span>02</span>
          <h3>Typed providers</h3>
          <p>Register dependencies by type with singleton, transient, request-scoped, lazy, optional, and factory patterns.</p>
        </article>
        <article>
          <span>03</span>
          <h3>HTTP boundary</h3>
          <p>Compose Axum routers, Tower layers, guards, validation pipes, OpenAPI metadata, and error envelopes.</p>
        </article>
        <article>
          <span>04</span>
          <h3>Runtime proof</h3>
          <p>Use CLI inspectors, TestApp, and the opt-in dashboard to verify route shape, module graph, OpenAPI output, and runtime behavior.</p>
        </article>
      </div>
    </section>

    <section class="surface-table" aria-labelledby="surfaces-title">
      <div>
        <p class="eyebrow">Framework surfaces</p>
        <h2 id="surfaces-title">The 1.0 surface is broad, but not blurry.</h2>
      </div>
      <div class="surface-list">
        ${surfaces.map(([title, slug, text]) => `<a href="${href(`${slug}/`)}">
          <strong>${title}</strong>
          <span>${text}</span>
        </a>`).join('')}
      </div>
    </section>

    <section class="adapter-story" aria-labelledby="adapter-title">
      <div>
        <p class="eyebrow">Lean core</p>
        <h2 id="adapter-title">The facade stays narrow. Adapters opt in.</h2>
        <p>Nidus does not smuggle vendor dependencies into every app. SQLx and cache support live in official crates with direct access to the underlying ecosystem clients.</p>
      </div>
      <div class="package-matrix" aria-label="Package groups">
        <span>nidus</span>
        <span>nidus-core</span>
        <span>nidus-http</span>
        <span>nidus-openapi</span>
        <span>nidus-validation</span>
        <span>nidus-dashboard</span>
        <span>nidus-sqlx</span>
        <span>nidus-cache</span>
        <span>cargo-nidus</span>
      </div>
    </section>

    <section class="example-panel" aria-labelledby="example-title">
      <div>
        <p class="eyebrow">Examples</p>
        <h2 id="example-title">Learn the framework from runnable services.</h2>
        <p>The examples move from a first controller to production-shaped services, adapter wiring, and external app templates. Use them as a guided tour of the public crates before choosing your own service shape.</p>
        <a class="text-link" href="${href('docs/examples/')}">View all examples</a>
      </div>
      <div class="example-paths">
        ${examplePaths.map(([title, text]) => `<article>
          <h3>${title}</h3>
          <p>${text}</p>
        </article>`).join('')}
      </div>
    </section>

    <section class="benchmark-teaser" aria-labelledby="benchmark-title">
      <div class="benchmark-teaser-copy">
        <p class="eyebrow">Benchmark evidence</p>
        <h2 id="benchmark-title">Measured against production-shaped peers, with bounded claims.</h2>
        <p>A homelab Kubernetes run compared Rust/Nidus, FastAPI, Spring Boot, and Express against the same PostgreSQL-backed endpoint contract. The run is intentionally conservative, but the latency story is clear.</p>
        <a class="text-link" href="${href('benchmarks/')}">Read the full benchmark breakdown</a>
      </div>
      <div class="benchmark-ledger" aria-label="Benchmark highlights">
        ${benchmarkHighlights.map(([label, value, detail]) => `<article>
          <span>${label}</span>
          <strong>${value}</strong>
          <p>${detail}</p>
        </article>`).join('')}
      </div>
    </section>

    <section class="proof-band" aria-labelledby="proof-title">
      <div class="section-heading">
        <p class="eyebrow">Release proof</p>
        <h2 id="proof-title">Trust comes from bounded claims.</h2>
      </div>
      <div class="proof-list">
        ${proof.map(([title, text]) => `<article><h3>${title}</h3><p>${text}</p></article>`).join('')}
      </div>
    </section>
  </main>`;
  return pageShell({
    title: 'Nidus',
    description: 'A modular Rust backend framework with typed DI, Axum routes, Tower middleware, explicit adapters, and production defaults.',
    body,
    currentSlug: '',
    home: true,
  });
}

function benchmarksPage() {
  const body = `<main class="benchmark-page">
    <section class="benchmark-hero" aria-labelledby="benchmark-page-title">
      <div>
        <p class="eyebrow">Benchmark evidence</p>
        <h1 id="benchmark-page-title">Nidus Framework Benchmark</h1>
        <p>Bounded homelab run comparing Rust/Nidus, FastAPI, Spring Boot, and Express with the same endpoint contract, PostgreSQL persistence, one replica, and a matched application deployment envelope.</p>
      </div>
      <aside class="benchmark-run-card" aria-label="Run summary">
        <span>Run timestamp</span>
        <strong>20260630T001754Z</strong>
        <p>k6 ran inside the benchmark namespace with 8 VUs and 20s per stack/profile. All checks passed, and every HTTP failure rate was 0.00%.</p>
      </aside>
    </section>

    <section class="benchmark-method" aria-labelledby="benchmark-method-title">
      <div>
        <p class="eyebrow">Method</p>
        <h2 id="benchmark-method-title">Bounded homelab run, not a max-throughput or capacity ceiling.</h2>
      </div>
      <div class="benchmark-method-grid">
        <article>
          <h3>Same contract</h3>
          <p>Each stack exposed ping, users, projects, events, search, and mixed-flow endpoints against PostgreSQL-backed application data.</p>
        </article>
        <article>
          <h3>Same envelope</h3>
          <p>Each app used one Kubernetes replica, ClusterIP service exposure, and the same application deployment shape.</p>
        </article>
        <article>
          <h3>Paced profile</h3>
          <p>The k6 harness used constant VUs and a short sleep per iteration to protect shared homelab workloads, so throughput is intentionally bounded.</p>
        </article>
      </div>
    </section>

    <section class="benchmark-results" aria-labelledby="benchmark-results-title">
      <div class="benchmark-results-heading">
        <div>
          <p class="eyebrow">Results</p>
          <h2 id="benchmark-results-title">Latency stayed low across every application-shaped profile.</h2>
        </div>
        <p>Rust/Nidus was the latency leader or effectively tied across the run. The mixed profile is close enough to treat as a tie with Java rather than a broad superiority claim.</p>
      </div>
      <div class="benchmark-table-wrap">
        <table class="benchmark-table">
          <thead>
            <tr>
              <th>Profile</th>
              <th>Stack</th>
              <th>req/s</th>
              <th>avg latency</th>
              <th>p95 latency</th>
              <th>failed</th>
              <th>checks</th>
            </tr>
          </thead>
          <tbody>
            ${benchmarkRows()}
          </tbody>
        </table>
      </div>
    </section>

    <section class="benchmark-interpretation" aria-labelledby="benchmark-interpretation-title">
      <div>
        <p class="eyebrow">Interpretation</p>
        <h2 id="benchmark-interpretation-title">What this proves, and what it does not.</h2>
      </div>
      <div class="benchmark-interpretation-list">
        <article>
          <h3>Supported claim</h3>
          <p>Under this conservative Kubernetes workload, Nidus completed the shared contract with zero request failures and consistently low latency.</p>
        </article>
        <article>
          <h3>Bounded claim</h3>
          <p>The req/s values are end-to-end paced workload results. They should not be presented as absolute capacity numbers.</p>
        </article>
        <article>
          <h3>Next proof layer</h3>
          <p>A capacity study would remove pacing, run longer windows, repeat samples, and isolate bottlenecks with continuous profiling.</p>
        </article>
      </div>
    </section>
  </main>`;

  return pageShell({
    title: 'Benchmarks',
    description: 'Nidus framework benchmark results from a bounded homelab Kubernetes run against FastAPI, Spring Boot, and Express.',
    body,
    currentSlug: 'benchmarks',
    standalone: true,
  });
}

function docPage(doc) {
  const markdown = loadDoc(doc);
  const content = markdownToHtml(markdown);
  return pageShell({
    title: doc.title,
    description: doc.summary,
    currentSlug: doc.slug,
    toc: extractToc(markdown),
    body: `<header class="doc-hero">
      <p class="doc-kicker">${escapeHtml(doc.group)}</p>
      <h1>${escapeHtml(doc.title)}</h1>
      <p>${escapeHtml(doc.summary)}</p>
    </header>
    <div class="doc-body">${content}</div>
    ${docsPager(doc.slug)}`,
  });
}

function notFoundPage() {
  const body = `<main class="not-found">
    <p class="eyebrow">404</p>
    <h1>Page not found</h1>
    <p>The Nidus documentation moved or the route is incomplete. Start at the docs index, installation guide, examples, or API reference.</p>
    <div class="hero-actions">
      <a class="button primary" href="${href('docs/')}">Docs</a>
      <a class="button secondary" href="${href('docs/installation/')}">Install</a>
      <a class="button ghost" href="${href('docs/examples/')}">Examples</a>
      <a class="button ghost" href="${href('docs/api-reference/')}">API</a>
    </div>
  </main>`;
  return pageShell({
    title: 'Page not found',
    description: 'This Nidus documentation route was not found. Continue to the Rust backend framework docs, installation guide, examples, or API reference.',
    body,
    currentSlug: '',
    home: true,
    noindex: true,
  });
}

function copyAsset(name) {
  fs.copyFileSync(path.join(ROOT, 'logos', name), path.join(DIST, 'assets', name));
}

function writeHtml(route, html) {
  const dir = path.join(DIST, route);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'index.html'), html);
}

function writeSeoFiles() {
  if (!SITE_ORIGIN) return;

  const pages = ['', 'benchmarks', ...docs.map((doc) => doc.slug)];
  const sitemapXml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${pages.map((page) => `  <url>
    <loc>${absoluteHref(page ? `${page}/` : '')}</loc>
  </url>`).join('\n')}
</urlset>
`;
  const robotsTxt = `User-agent: *
Allow: /

Sitemap: ${absoluteHref('sitemap.xml')}
`;

  fs.writeFileSync(path.join(DIST, 'sitemap.xml'), sitemapXml);
  fs.writeFileSync(path.join(DIST, 'robots.txt'), robotsTxt);
}

function main() {
  fs.rmSync(DIST, { recursive: true, force: true });
  fs.mkdirSync(path.join(DIST, 'assets'), { recursive: true });
  for (const assetName of [
    'logo-mark-transparent.png',
    'logo-full-transparent.png',
    'site-logo-dark.png',
    'site-logo-light.png',
    'favicon-32.png',
    'apple-touch-icon.png',
    'og-image.png',
  ]) copyAsset(assetName);
  fs.copyFileSync(path.join(SRC, 'styles.css'), path.join(DIST, 'styles.css'));
  fs.copyFileSync(path.join(SRC, 'app.js'), path.join(DIST, 'app.js'));

  fs.writeFileSync(path.join(DIST, 'index.html'), homePage());
  writeHtml('benchmarks', benchmarksPage());
  for (const doc of docs) {
    writeHtml(doc.slug, docPage(doc));
  }
  fs.writeFileSync(path.join(DIST, '404.html'), notFoundPage());
  if (SITE_DOMAIN) {
    fs.writeFileSync(path.join(DIST, 'CNAME'), `${SITE_DOMAIN}\n`);
  }
  writeSeoFiles();
  fs.writeFileSync(path.join(DIST, 'site-map.json'), JSON.stringify({ base: BASE, domain: SITE_DOMAIN || null, pages: ['', 'benchmarks', ...docs.map((doc) => doc.slug)] }, null, 2));
  console.log(`Built Nidus site at ${path.relative(ROOT, DIST)} with base ${BASE}${SITE_DOMAIN ? ` and domain ${SITE_DOMAIN}` : ''}`);
}

main();
