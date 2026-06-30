#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.resolve(__dirname, '../dist');
const STYLES = path.resolve(__dirname, '../src/styles.css');
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

const home = readDist('/');
const benchmarks = readDist('benchmarks');
const siteMapPath = path.join(DIST, 'site-map.json');
const siteMap = fs.existsSync(siteMapPath) ? JSON.parse(fs.readFileSync(siteMapPath, 'utf8')) : { pages: [] };
const css = fs.existsSync(STYLES) ? fs.readFileSync(STYLES, 'utf8') : '';

assertIncludes('/', home, 'Benchmark evidence');
assertIncludes('/', home, '/benchmarks/');
assertIncludes('/', home, '423.72us');
assertIncludes('/', home, '0.00% failed');

assertIncludes('benchmarks', benchmarks, 'Nidus Framework Benchmark');
assertIncludes('benchmarks', benchmarks, 'Bounded homelab run');
assertIncludes('benchmarks', benchmarks, 'rust-nidus');
assertIncludes('benchmarks', benchmarks, 'python-fastapi');
assertIncludes('benchmarks', benchmarks, 'java-spring');
assertIncludes('benchmarks', benchmarks, 'node-express');
assertIncludes('benchmarks', benchmarks, '423.939646/s');
assertIncludes('benchmarks', benchmarks, 'not a max-throughput or capacity ceiling');

for (const route of ['/', 'benchmarks']) {
  const html = route === '/' ? home : benchmarks;
  assertExcludes(route, html, 'Resource Consumption');
  assertExcludes(route, html, 'Peak CPU');
  assertExcludes(route, html, 'Peak memory');
  assertExcludes(route, html, '2Mi');
  assertExcludes(route, html, '44m');
}

if (!siteMap.pages.includes('benchmarks')) failures.push('site-map.json: benchmarks route is missing');

const bodyRule = extractRuleBlock(css, 'body');
const ledgerRule = extractRuleBlock(css, '.benchmark-ledger');
const ledgerArticleRule = extractRuleBlock(css, '.benchmark-ledger article');
const ledgerValueRule = extractRuleBlock(css, '.benchmark-ledger strong');
const mobile840 = extractMediaBlock(css, 'max-width: 840px');
const mobile520 = extractMediaBlock(css, 'max-width: 520px');
const mobileLedgerArticleRule = extractRuleBlock(mobile840, '.benchmark-ledger article');
const mobileLedgerValueRule = extractRuleBlock(mobile840, '.benchmark-ledger strong');

assertCssIncludes('body overflow guard', bodyRule, 'overflow-x: clip;');
assertCssIncludes('teaser responsive columns', css, '.benchmark-teaser {\n  display: grid;\n  grid-template-columns: minmax(280px, 0.52fr) minmax(0, 0.9fr);');
assertCssIncludes('ledger clipping guard', ledgerRule, 'overflow: hidden;');
assertCssIncludes('ledger article flexible text column', ledgerArticleRule, 'grid-template-columns: minmax(124px, 0.24fr) minmax(0, 1fr);');
assertCssIncludes('ledger value stable digits', ledgerValueRule, 'font-variant-numeric: tabular-nums;');
assertCssIncludes('ledger value desktop no-wrap', ledgerValueRule, 'white-space: nowrap;');
assertCssIncludes('mobile layout includes benchmark teaser', mobile840, '.benchmark-teaser,');
assertCssIncludes('mobile benchmark article single column', mobileLedgerArticleRule, 'grid-template-columns: 1fr;');
assertCssIncludes('mobile benchmark value wraps', mobileLedgerValueRule, 'white-space: normal;');
assertCssIncludes('mobile benchmark value overflow guard', mobileLedgerValueRule, 'overflow-wrap: anywhere;');
assertCssIncludes('small viewport includes benchmark teaser', mobile520, '.benchmark-teaser,');

if (failures.length) {
  console.error('Benchmark content check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('Checked public benchmark route, homepage entry point, nav links, omitted resource usage wording, and responsive CSS guards.');
