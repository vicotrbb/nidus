#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.resolve(__dirname, '../dist');
const STYLES = path.resolve(__dirname, '../src/styles.css');
const DATA = path.resolve(__dirname, '../data/benchmarks');
const failures = [];

function readDist(route) {
  const file = route === '/' ? path.join(DIST, 'index.html') : path.join(DIST, route, 'index.html');
  if (!fs.existsSync(file)) {
    failures.push(`${route}: missing generated HTML`);
    return '';
  }
  return fs.readFileSync(file, 'utf8');
}

function assertIncludes(route, html, value) {
  if (!html.includes(value)) failures.push(`${route}: missing ${value}`);
}

function assertExcludes(route, html, value) {
  if (html.toLowerCase().includes(value.toLowerCase())) failures.push(`${route}: forbidden resource-usage wording ${value}`);
}

function assertExcludesPattern(route, html, pattern, label) {
  if (pattern.test(html)) failures.push(`${route}: forbidden resource-usage wording ${label}`);
}

function assertCssIncludes(label, css, value) {
  if (!css.includes(value)) failures.push(`styles.css ${label}: missing ${value}`);
}

function extractMediaBlock(css, query) {
  const mediaStart = css.indexOf(`@media (${query})`);
  if (mediaStart === -1) return '';

  const blockStart = css.indexOf('{', mediaStart);
  if (blockStart === -1) return '';

  let depth = 0;
  for (let index = blockStart; index < css.length; index += 1) {
    const char = css[index];
    if (char === '{') depth += 1;
    if (char === '}') depth -= 1;
    if (depth === 0) return css.slice(blockStart + 1, index);
  }

  return '';
}

function extractRuleBlock(css, selector) {
  const selectorStart = css.indexOf(selector);
  if (selectorStart === -1) return '';

  const blockStart = css.indexOf('{', selectorStart);
  if (blockStart === -1) return '';

  const blockEnd = css.indexOf('}', blockStart);
  if (blockEnd === -1) return '';

  return css.slice(blockStart + 1, blockEnd);
}

function filesRecursively(directory, prefix = '') {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const relative = path.join(prefix, entry.name);
    return entry.isDirectory()
      ? filesRecursively(path.join(directory, entry.name), relative)
      : [relative];
  });
}

function verifyManifest(runRoot) {
  const manifestPath = path.join(runRoot, 'MANIFEST.sha256');
  const entries = fs.readFileSync(manifestPath, 'utf8').trim().split('\n').map((line) => {
    const match = line.match(/^([0-9a-f]{64})  (.+)$/);
    if (!match) {
      failures.push(`benchmark manifest: malformed line ${line}`);
      return null;
    }
    return { digest: match[1], relative: match[2] };
  }).filter(Boolean);
  const listed = new Set(entries.map((entry) => entry.relative));
  const actualFiles = filesRecursively(runRoot)
    .filter((relative) => relative !== 'MANIFEST.sha256');

  if (listed.size !== entries.length) failures.push('benchmark manifest: duplicate entries are present');
  if (actualFiles.length !== entries.length) failures.push(`benchmark manifest: expected ${actualFiles.length} file entries, found ${entries.length}`);
  for (const relative of actualFiles) {
    if (!listed.has(relative)) failures.push(`benchmark manifest: unlisted file ${relative}`);
  }
  for (const entry of entries) {
    const file = path.join(runRoot, entry.relative);
    if (!fs.existsSync(file)) {
      failures.push(`benchmark manifest: missing ${entry.relative}`);
      continue;
    }
    const digest = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    if (digest !== entry.digest) failures.push(`benchmark manifest: digest mismatch for ${entry.relative}`);
  }
}

const home = readDist('/');
const benchmarks = readDist('benchmarks');
const currentPath = path.join(DATA, 'v1.0.12/summary.json');
const historicalPath = path.join(DATA, 'v1.0.4/summary.json');
const current = JSON.parse(fs.readFileSync(currentPath, 'utf8'));
const historical = JSON.parse(fs.readFileSync(historicalPath, 'utf8'));
const siteMapPath = path.join(DIST, 'site-map.json');
const siteMap = fs.existsSync(siteMapPath) ? JSON.parse(fs.readFileSync(siteMapPath, 'utf8')) : { pages: [] };
const css = fs.existsSync(STYLES) ? fs.readFileSync(STYLES, 'utf8') : '';

assertIncludes('/', home, 'Benchmark evidence');
assertIncludes('/', home, '/benchmarks/');
assertIncludes('/', home, '1.0.12 candidate');
assertIncludes('/', home, '0.00% failed');

assertIncludes('benchmarks', benchmarks, 'Nidus benchmarks, version by version.');
assertIncludes('benchmarks', benchmarks, current.run.id);
assertIncludes('benchmarks', benchmarks, '1.0.4 → 1.0.12');
assertIncludes('benchmarks', benchmarks, 'rust-nidus 1.0.12');
assertIncludes('benchmarks', benchmarks, 'python-fastapi');
assertIncludes('benchmarks', benchmarks, 'java-spring');
assertIncludes('benchmarks', benchmarks, 'node-express');
assertIncludes('benchmarks', benchmarks, '443.34/s');
assertIncludes('benchmarks', benchmarks, 'Qualified, not strictly accepted');
assertIncludes('benchmarks', benchmarks, '15.58%');
assertIncludes('benchmarks', benchmarks, 'No sample was discarded');
assertIncludes('benchmarks', benchmarks, '/benchmark-data/v1.0.12/summary.json');
assertIncludes('benchmarks', benchmarks, '/benchmark-data/v1.0.12/run/MANIFEST.sha256');
assertIncludes('benchmarks', benchmarks, 'not a max-throughput or capacity ceiling');

if (current.status !== 'qualified'
  || current.quality.accepted !== false
  || current.quality.publicationEligible !== true) {
  failures.push('v1.0.12 summary: expected an explicit publication-eligible qualification');
}
if (current.quality.failures.length !== 1
  || current.quality.failures[0] !== 'java-spring/ping: average latency CV 15.58% exceeded 15%') {
  failures.push('v1.0.12 summary: exact Spring repeatability qualification is not preserved');
}
if (current.quality.qualification?.samplesDiscarded !== 0
  || current.quality.qualification?.thresholdsChanged !== false
  || current.quality.qualification?.failures?.length !== 1) {
  failures.push('v1.0.12 summary: qualification boundaries are incomplete');
}
if (current.quality.verifiedK6ExecutionCount !== current.quality.rawSummaryCount
  || current.quality.verifiedK6ExecutionCount !== 179
  || current.quality.verifiedK6PodCount !== 3) {
  failures.push('v1.0.12 summary: stage-pinned k6 execution evidence is incomplete');
}
if (current.quality.baseMeasuredSummaryCount !== 75
  || current.quality.adaptiveMeasuredSummaryCount !== 12
  || current.quality.measuredSummaryCount !== 87
  || current.quality.measuredSummaryCount !== current.quality.baseMeasuredSummaryCount + current.quality.adaptiveMeasuredSummaryCount) {
  failures.push('v1.0.12 summary: base and adaptive measurement counts do not reconcile');
}
if (current.aggregates.length !== 25) failures.push(`v1.0.12 summary: expected 25 aggregates, found ${current.aggregates.length}`);
if (current.crossFrameworkRows.length !== 20) failures.push(`v1.0.12 summary: expected 20 cross-framework rows, found ${current.crossFrameworkRows.length}`);
if (current.versionComparison.length !== 5) failures.push(`v1.0.12 summary: expected 5 comparison rows, found ${current.versionComparison.length}`);
if (historical.rows.length !== 20) failures.push(`v1.0.4 summary: expected 20 historical rows, found ${historical.rows.length}`);
if (historical.comparability.eligibleForVersionComparison !== false) failures.push('v1.0.4 summary: historical snapshot must be excluded from version deltas');

for (const result of current.aggregates) {
  if (result.median.httpFailureRate !== 0 || result.median.checkRate !== 1) {
    failures.push(`v1.0.12 summary: ${result.stack}/${result.profile} failed correctness gates`);
  }
  if (result.stack.startsWith('rust-nidus-')) {
    const thresholds = current.quality.thresholds;
    if (result.coefficientOfVariationPercent.throughput > thresholds.throughputCoefficientOfVariationPercent
      || result.coefficientOfVariationPercent.averageLatency > thresholds.averageLatencyCoefficientOfVariationPercent
      || result.coefficientOfVariationPercent.p95Latency > thresholds.p95LatencyCoefficientOfVariationPercent) {
      failures.push(`v1.0.12 summary: Nidus row ${result.stack}/${result.profile} exceeded a repeatability threshold`);
    }
  }
}

const qualifiedRow = current.aggregates.find((result) => result.stack === 'java-spring' && result.profile === 'ping');
if (qualifiedRow?.sampleCount !== 9 || qualifiedRow?.coefficientOfVariationPercent.averageLatency !== 15.579) {
  failures.push('v1.0.12 summary: qualified Spring row does not retain all nine samples and exact CV');
}
const mixedComparison = current.versionComparison.find((result) => result.profile === 'mixed');
if (mixedComparison?.previousSampleCount !== 6 || mixedComparison?.currentSampleCount !== 3) {
  failures.push('v1.0.12 summary: mixed version comparison sample counts are incomplete');
}

for (const relative of ['v1.0.12/summary.json', 'v1.0.12/run/MANIFEST.sha256', 'v1.0.4/summary.json']) {
  const published = path.join(DIST, 'benchmark-data', relative);
  if (!fs.existsSync(published)) failures.push(`benchmark-data: missing published ${relative}`);
}

const publicRun = path.join(DATA, 'v1.0.12/run');
verifyManifest(publicRun);
const publicResultFiles = filesRecursively(publicRun);
for (const relative of publicResultFiles) {
  if (/(^|\/)(?:workload\.js|reset-schema\.sql|truncate-schema\.sql|Dockerfile)$/.test(relative)
    || /\.(?:rs|sh|mjs|tar)$/.test(relative)) {
    failures.push(`benchmark-data: forbidden benchmark code or build artifact ${relative}`);
  }
  const contents = fs.readFileSync(path.join(publicRun, relative), 'utf8');
  if (/\b10(?:\.\d{1,3}){3}\b|\b172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2}\b|\b192\.168(?:\.\d{1,3}){2}\b/.test(contents)) {
    failures.push(`benchmark-data: private address remained in ${relative}`);
  }
}

for (const route of ['/', 'benchmarks']) {
  const html = route === '/' ? home : benchmarks;
  assertExcludes(route, html, 'Resource Consumption');
  assertExcludes(route, html, 'Peak CPU');
  assertExcludes(route, html, 'Peak memory');
  assertExcludesPattern(route, html, /(?:^|[>\s])2Mi(?:$|[<\s])/i, 'standalone 2Mi');
  assertExcludesPattern(route, html, /\b44m\b/i, 'standalone 44m');
}

if (!siteMap.pages.includes('benchmarks')) failures.push('site-map.json: benchmarks route is missing');

const bodyRule = extractRuleBlock(css, 'body');
const ledgerRule = extractRuleBlock(css, '.benchmark-ledger');
const ledgerArticleRule = extractRuleBlock(css, '.benchmark-ledger article');
const ledgerValueRule = extractRuleBlock(css, '.benchmark-ledger strong');
const versionNavRule = extractRuleBlock(css, '.benchmark-version-nav');
const qualificationRule = extractRuleBlock(css, '.benchmark-qualification');
const mobile840 = extractMediaBlock(css, 'max-width: 840px');
const mobile520 = extractMediaBlock(css, 'max-width: 520px');
const mobileLedgerArticleRule = extractRuleBlock(mobile840, '.benchmark-ledger article');
const mobileLedgerValueRule = extractRuleBlock(mobile840, '.benchmark-ledger strong');
const mobileVersionNavRule = extractRuleBlock(mobile840, '.benchmark-version-nav');

assertCssIncludes('body overflow guard', bodyRule, 'overflow-x: clip;');
assertCssIncludes('teaser responsive columns', css, '.benchmark-teaser {\n  display: grid;\n  grid-template-columns: minmax(280px, 0.52fr) minmax(0, 0.9fr);');
assertCssIncludes('ledger clipping guard', ledgerRule, 'overflow: hidden;');
assertCssIncludes('ledger article flexible text column', ledgerArticleRule, 'grid-template-columns: minmax(124px, 0.24fr) minmax(0, 1fr);');
assertCssIncludes('ledger value stable digits', ledgerValueRule, 'font-variant-numeric: tabular-nums;');
assertCssIncludes('ledger value desktop no-wrap', ledgerValueRule, 'white-space: nowrap;');
assertCssIncludes('version navigation columns', versionNavRule, 'grid-template-columns: repeat(3, minmax(0, 1fr));');
assertCssIncludes('qualification evidence layout', qualificationRule, 'grid-template-columns: minmax(190px, 0.28fr) minmax(0, 1fr);');
assertCssIncludes('mobile layout includes benchmark teaser', mobile840, '.benchmark-teaser,');
assertCssIncludes('mobile benchmark article single column', mobileLedgerArticleRule, 'grid-template-columns: 1fr;');
assertCssIncludes('mobile benchmark value wraps', mobileLedgerValueRule, 'white-space: normal;');
assertCssIncludes('mobile benchmark value overflow guard', mobileLedgerValueRule, 'overflow-wrap: anywhere;');
assertCssIncludes('mobile version navigation single column', mobileVersionNavRule, 'grid-template-columns: 1fr;');
assertCssIncludes('mobile qualification single column', mobile840, '.benchmark-qualification {\n    grid-template-columns: 1fr;');
assertCssIncludes('small viewport includes benchmark teaser', mobile520, '.benchmark-teaser,');

if (failures.length) {
  console.error('Benchmark content check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('Checked qualified versioned benchmark data, manifest integrity, results-only boundaries, public tables, evidence links, and responsive CSS guards.');
