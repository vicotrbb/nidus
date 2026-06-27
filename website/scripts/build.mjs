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

const docs = [
  {
    title: 'Docs',
    slug: 'docs',
    group: 'Start',
    source: 'docs/README.md',
    summary: 'Guide index for the Nidus 1.0 framework surface.',
  },
  {
    title: 'Getting Started',
    slug: 'docs/getting-started',
    group: 'Start',
    source: 'docs/getting-started.md',
    summary: 'Create and inspect a Nidus application.',
  },
  {
    title: 'Installation',
    slug: 'docs/installation',
    group: 'Start',
    markdown: `# Installation

Install the Nidus CLI after the 1.0 crates are published:

\`\`\`bash
cargo install cargo-nidus
cargo nidus new hello-nidus
\`\`\`

During local framework development, install directly from this checkout:

\`\`\`bash
cargo install --path crates/cargo-nidus
cargo nidus new hello-nidus
\`\`\`

Applications depend on the facade crate and opt into feature groups explicitly:

\`\`\`toml
[dependencies]
nidus = { version = "1.0", features = ["http", "config", "openapi", "validation"] }
\`\`\`

Official adapters are separate crates, so the core facade stays lean:

\`\`\`toml
nidus-sqlx = { version = "1.0", features = ["sqlite"] }
nidus-cache = { version = "1.0", features = ["moka"] }
\`\`\`

The current repository state has local package dry-run proof. Publishing still requires crates.io credentials and the correct dependency-order publish sequence.`,
    summary: 'Install the CLI, facade crate, and optional adapters.',
  },
  {
    title: 'CLI',
    slug: 'docs/cli',
    group: 'Start',
    markdown: `# CLI

\`cargo-nidus\` provides project generation and source inspection commands:

\`\`\`bash
cargo nidus new hello-nidus
cargo nidus check
cargo nidus routes
cargo nidus graph
cargo nidus openapi
cargo nidus expand --dry-run
\`\`\`

Use \`check\` before committing generated applications. It validates expected project files, module indexes, and generated feature declarations.

Use \`routes\`, \`graph\`, and \`openapi\` to inspect the Rust source that Nidus macros annotate. These commands are intentionally source-driven and keep framework behavior inspectable.`,
    summary: 'Project generation and inspection commands.',
  },
  { title: 'Mental Model', slug: 'docs/mental-model', group: 'Concepts', source: 'docs/mental-model.md', summary: 'How Nidus maps modules, providers, controllers, guards, and pipes to Rust.' },
  { title: 'Architecture', slug: 'docs/architecture', group: 'Concepts', source: 'docs/architecture.md', summary: 'Workspace crates and dependency boundaries.' },
  { title: 'Modules', slug: 'docs/modules', group: 'Concepts', source: 'docs/modules.md', summary: 'Module imports, providers, controllers, exports, and graph validation.' },
  { title: 'Providers / DI', slug: 'docs/providers-di', group: 'Concepts', source: 'docs/dependency-injection.md', summary: 'Typed dependency injection, factories, optional dependencies, and request scope.' },
  { title: 'Providers', slug: 'docs/providers', group: 'Concepts', source: 'docs/providers.md', summary: 'Provider design, lifetimes, and registration patterns.' },
  { title: 'Controllers / Routes', slug: 'docs/controllers-routes', group: 'HTTP', source: 'docs/controllers.md', summary: 'Controller macros, route definitions, metadata, and Axum composition.' },
  { title: 'Guards', slug: 'docs/guards', group: 'HTTP', source: 'docs/guards.md', summary: 'Authorization guards and guard layers.' },
  { title: 'Pipes / Validation', slug: 'docs/pipes-validation', group: 'HTTP', source: 'docs/pipes.md', summary: 'Validation pipes, DTO validation, and stable 422 responses.' },
  { title: 'Interceptors', slug: 'docs/interceptors', group: 'HTTP', source: 'docs/interceptors.md', summary: 'Tower-first interception and middleware guidance.' },
  { title: 'Error Handling', slug: 'docs/error-handling', group: 'HTTP', source: 'docs/error-handling.md', summary: 'HTTP errors and production error envelopes.' },
  { title: 'Config', slug: 'docs/config', group: 'Runtime', source: 'docs/config.md', summary: 'Typed config from pairs, JSON, files, and environment variables.' },
  { title: 'OpenAPI', slug: 'docs/openapi', group: 'Runtime', source: 'docs/openapi.md', summary: 'OpenAPI route metadata and document rendering.' },
  { title: 'Events', slug: 'docs/events', group: 'Runtime', source: 'docs/events.md', summary: 'In-process event bus and observed events.' },
  { title: 'Jobs', slug: 'docs/jobs', group: 'Runtime', source: 'docs/jobs.md', summary: 'Sync and async job queues with observed runners.' },
  { title: 'Testing', slug: 'docs/testing', group: 'Runtime', source: 'docs/testing.md', summary: 'TestApp request helpers and provider overrides.' },
  { title: 'Integrations / Adapters', slug: 'docs/integrations-adapters', group: 'Ecosystem', source: 'docs/integrations.md', summary: 'Separately installable SQLx and cache adapters.' },
  { title: 'Production / Deployment', slug: 'docs/production-deployment', group: 'Ecosystem', source: 'docs/deployment.md', summary: 'Production defaults, logging, OTel helpers, health, and deployment boundaries.' },
  { title: 'Examples', slug: 'docs/examples', group: 'Ecosystem', source: 'docs/examples.md', summary: 'Workspace examples and validation commands.' },
  { title: 'Performance', slug: 'docs/performance', group: 'Ecosystem', source: 'docs/performance.md', summary: 'Benchmark surfaces and local result boundaries.' },
  { title: 'Migration From NestJS', slug: 'docs/migration-from-nestjs', group: 'Ecosystem', source: 'docs/migration-from-nestjs.md', summary: 'Concept mapping without runtime metadata cloning.' },
  {
    title: 'API Reference',
    slug: 'docs/api-reference',
    group: 'Reference',
    markdown: apiReference(),
    summary: 'Crate map and generated Rust API reference entry points.',
  },
  {
    title: 'Release 1.0',
    slug: 'docs/release-1-0',
    group: 'Reference',
    markdown: releaseNotes(),
    summary: 'Nidus 1.0 release notes and proof boundaries.',
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

function apiReference() {
  const crates = [
    ['nidus', 'Facade crate and prelude'],
    ['nidus-core', 'Modules, DI, lifecycle, and app bootstrap'],
    ['nidus-http', 'Controllers, routing, middleware, health, metrics, logging, OTel helpers'],
    ['nidus-macros', 'Controller, route, module, provider, guard, pipe, and entrypoint macros'],
    ['nidus-config', 'Typed configuration values and loaders'],
    ['nidus-openapi', 'OpenAPI route metadata and document generation'],
    ['nidus-validation', 'Validation pipes and JSON extractors backed by garde'],
    ['nidus-auth', 'Guard traits, combinators, and Tower layers'],
    ['nidus-events', 'Event bus and observed event dispatch'],
    ['nidus-jobs', 'Job queues and observed job runners'],
    ['nidus-testing', 'TestApp request harness and provider overrides'],
    ['nidus-sqlx', 'Official SQLx adapter'],
    ['nidus-cache', 'Official Moka cache adapter'],
    ['cargo-nidus', 'CLI generator and source inspector'],
  ];
  const rows = crates.map(([name, summary]) => `| \`${name}\` | ${summary} | https://docs.rs/${name}/1.0.0/${name.replaceAll('-', '_')}/ |`).join('\n');
  return `# API Reference

The release website links to generated Rust API references on docs.rs once the crates are published. During local launch verification, build the same reference set with:

\`\`\`bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
\`\`\`

| Crate | Surface | Reference |
| --- | --- | --- |
${rows}

The facade crate keeps core Nidus ergonomic, while SQLx and cache integrations remain separate installable crates.`;
}

function releaseNotes() {
  return `# Release 1.0

Nidus 1.0 is the first stable release target for the framework surface in this repository.

## Highlights

- Modular Rust application structure with modules, controllers, providers, guards, validation, config, OpenAPI, events, jobs, testing, and production HTTP defaults.
- Lean \`nidus\` facade with SQLx and cache integrations delivered as separately installable official adapters.
- \`cargo-nidus\` project generation plus source inspection commands for routes, module graphs, macro expansion, checks, and OpenAPI.
- Validation now uses \`garde\`, removing the unmaintained \`proc-macro-error2\` advisory path without suppressing the advisory.
- Logo assets and documentation website are generated from repository sources.

## Proof Boundary

Local verification can prove package dry-runs, tests, docs, website output, link checks, and runtime examples. Actual crates.io publishing and GitHub Pages deployment require credentials or repository settings outside local code execution, so those steps must be reported separately when blocked.`;
}

function loadDoc(doc) {
  return doc.markdown ?? read(doc.source);
}

function pageShell({ title, description, body, currentSlug, home = false }) {
  const docLinks = docs.map((doc) => `<a href="${href(`${doc.slug}/`)}" ${doc.slug === currentSlug ? 'aria-current="page"' : ''}>${doc.title}</a>`).join('');
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(title)} · Nidus</title>
  <meta name="description" content="${escapeHtml(description)}">
  <meta property="og:title" content="${escapeHtml(title)} · Nidus">
  <meta property="og:description" content="${escapeHtml(description)}">
  <meta property="og:image" content="${asset('og-image.png')}">
  <link rel="icon" href="${asset('favicon-32.png')}" sizes="32x32">
  <link rel="apple-touch-icon" href="${asset('apple-touch-icon.png')}">
  <link rel="stylesheet" href="${href('styles.css')}">
</head>
<body class="${home ? 'home' : 'doc-page'}">
  <header class="site-header">
    <a class="brand" href="${href()}">
      <img src="${asset('logo-mark-transparent.png')}" alt="" width="42" height="32">
      <span>Nidus</span>
    </a>
    <button class="nav-toggle" type="button" aria-expanded="false" aria-controls="site-nav">Menu</button>
    <nav id="site-nav" class="site-nav" aria-label="Primary">
      <a href="${href('docs/')}">Docs</a>
      <a href="${href('docs/installation/')}">Install</a>
      <a href="${href('docs/examples/')}">Examples</a>
      <a href="${href('docs/api-reference/')}">API</a>
      <a href="https://github.com/victorbona/nidus">Source</a>
    </nav>
  </header>
  ${home ? body : `<main class="docs-layout">
    <aside class="docs-sidebar">
      <label for="docs-filter">Search docs</label>
      <input id="docs-filter" type="search" placeholder="Filter pages">
      <nav class="docs-links" aria-label="Documentation">${docLinks}</nav>
    </aside>
    <article class="doc-content">${body}</article>
  </main>`}
  <footer class="site-footer">
    <span>Nidus 1.0</span>
    <a href="${href('docs/release-1-0/')}">Release notes</a>
    <a href="${href('docs/production-deployment/')}">Production</a>
    <a href="${href('docs/integrations-adapters/')}">Adapters</a>
  </footer>
  <script src="${href('app.js')}" type="module"></script>
</body>
</html>`;
}

function homePage() {
  const pillars = [
    ['Organized Rust services', 'Modules, providers, controllers, guards, pipes, and OpenAPI metadata without runtime reflection.'],
    ['Lean core, explicit adapters', 'SQLx and cache support ship as separate crates so backend vendors only compile when installed.'],
    ['Production HTTP defaults', 'Request IDs, request context, health, metrics, CORS, body limits, timeouts, error envelopes, and structured tracing stay visible.'],
    ['Proof-oriented workflow', 'CLI inspection, TestApp, compile-fail coverage, package dry-runs, and docs checks make release claims auditable.'],
  ];
  const docCards = docs.filter((doc) => ['docs/getting-started', 'docs/installation', 'docs/modules', 'docs/controllers-routes', 'docs/integrations-adapters', 'docs/production-deployment'].includes(doc.slug));
  const body = `<main>
    <section class="hero">
      <div class="hero-copy">
        <p class="eyebrow">Rust backend framework · 1.0 release track</p>
        <h1>Nidus</h1>
        <p class="hero-text">NestJS-like application structure for Rust teams who want typed dependency injection, explicit modules, Axum routes, Tower middleware, and production defaults without hiding the ecosystem.</p>
        <div class="hero-actions">
          <a class="button primary" href="${href('docs/installation/')}">Install</a>
          <a class="button secondary" href="${href('docs/')}">Read docs</a>
        </div>
        <pre class="install-snippet"><code>cargo install cargo-nidus
cargo nidus new hello-nidus
cd hello-nidus
cargo run</code></pre>
      </div>
      <div class="hero-mark" aria-hidden="true">
        <img src="${asset('logo-mark-transparent.png')}" alt="">
      </div>
    </section>

    <section class="quick-map" aria-label="Nidus feature map">
      ${pillars.map(([title, text]) => `<article><h2>${title}</h2><p>${text}</p></article>`).join('')}
    </section>

    <section class="docs-preview">
      <div class="section-heading">
        <p class="eyebrow">Documentation-first</p>
        <h2>Everything needed to evaluate the framework is one click away.</h2>
      </div>
      <div class="doc-card-grid">
        ${docCards.map((doc) => `<a class="doc-card" href="${href(`${doc.slug}/`)}"><span>${doc.group}</span><strong>${doc.title}</strong><p>${doc.summary}</p></a>`).join('')}
      </div>
    </section>

    <section class="ecosystem">
      <div>
        <p class="eyebrow">Facade plus adapters</p>
        <h2>Core Nidus stays narrow.</h2>
      </div>
      <div class="package-strip" aria-label="Package groups">
        <span>nidus</span>
        <span>nidus-core</span>
        <span>nidus-http</span>
        <span>nidus-openapi</span>
        <span>nidus-validation</span>
        <span>nidus-sqlx</span>
        <span>nidus-cache</span>
        <span>cargo-nidus</span>
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

function docPage(doc) {
  const content = markdownToHtml(loadDoc(doc));
  return pageShell({
    title: doc.title,
    description: doc.summary,
    currentSlug: doc.slug,
    body: `<div class="doc-kicker">${escapeHtml(doc.group)}</div>${content}`,
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
  for (const doc of docs) {
    writeHtml(doc.slug, docPage(doc));
  }
  fs.writeFileSync(path.join(DIST, 'site-map.json'), JSON.stringify({ base: BASE, pages: ['', ...docs.map((doc) => doc.slug)] }, null, 2));
  console.log(`Built Nidus site at ${path.relative(ROOT, DIST)} with base ${BASE}`);
}

main();
