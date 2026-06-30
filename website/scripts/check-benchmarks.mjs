#!/usr/bin/env node
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.resolve(__dirname, '../dist');
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

const home = readDist('/');
const benchmarks = readDist('benchmarks');
const siteMapPath = path.join(DIST, 'site-map.json');
const siteMap = fs.existsSync(siteMapPath) ? JSON.parse(fs.readFileSync(siteMapPath, 'utf8')) : { pages: [] };
const base = siteMap.base === '/' ? '/' : siteMap.base;

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

function contentType(file) {
  if (file.endsWith('.html')) return 'text/html; charset=utf-8';
  if (file.endsWith('.css')) return 'text/css; charset=utf-8';
  if (file.endsWith('.js')) return 'text/javascript; charset=utf-8';
  if (file.endsWith('.png')) return 'image/png';
  return 'application/octet-stream';
}

function serveDist() {
  const server = http.createServer((request, response) => {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1');
    let pathname = decodeURIComponent(url.pathname);
    if (base !== '/' && pathname.startsWith(base)) {
      pathname = `/${pathname.slice(base.length)}`;
    }
    const normalized = pathname === '/' ? '/index.html' : pathname;
    const candidate = path.normalize(path.join(DIST, normalized));
    if (!candidate.startsWith(DIST)) {
      response.writeHead(403).end();
      return;
    }
    const file = fs.existsSync(candidate) && fs.statSync(candidate).isDirectory()
      ? path.join(candidate, 'index.html')
      : candidate;
    if (!fs.existsSync(file)) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { 'content-type': contentType(file) });
    fs.createReadStream(file).pipe(response);
  });

  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      resolve({ server, url: `http://127.0.0.1:${address.port}/` });
    });
  });
}

async function checkHomepageBenchmarkLayout() {
  const { chromium } = await import(process.env.PLAYWRIGHT_MODULE_PATH ?? 'playwright');
  const { server, url } = await serveDist();
  const browser = await chromium.launch({ headless: true });

  try {
    for (const viewport of [
      { name: 'desktop', width: 1440, height: 1000 },
      { name: 'tablet', width: 820, height: 1180 },
      { name: 'phone', width: 390, height: 844 },
      { name: 'narrow phone', width: 320, height: 568 },
    ]) {
      const page = await browser.newPage({ viewport });
      await page.goto(new URL(base, url).href, { waitUntil: 'networkidle' });
      const result = await page.evaluate(() => {
        const section = document.querySelector('.benchmark-teaser');
        const ledger = document.querySelector('.benchmark-ledger');
        const articleResults = [...document.querySelectorAll('.benchmark-ledger article')].map((article) => {
          const articleBox = article.getBoundingClientRect();
          const value = article.querySelector('strong');
          const detail = article.querySelector('p');
          const valueBox = value?.getBoundingClientRect();
          const detailBox = detail?.getBoundingClientRect();
          return {
            article: {
              left: articleBox.left,
              right: articleBox.right,
              width: articleBox.width,
              scrollWidth: article.scrollWidth,
              clientWidth: article.clientWidth,
            },
            value: valueBox ? { left: valueBox.left, right: valueBox.right, top: valueBox.top, bottom: valueBox.bottom, width: valueBox.width } : null,
            detail: detailBox ? { left: detailBox.left, right: detailBox.right, top: detailBox.top, bottom: detailBox.bottom, width: detailBox.width } : null,
          };
        });
        const sectionBox = section?.getBoundingClientRect();
        const ledgerBox = ledger?.getBoundingClientRect();
        return {
          viewportWidth: innerWidth,
          pageWidth: document.documentElement.scrollWidth,
          section: sectionBox ? { left: sectionBox.left, right: sectionBox.right } : null,
          ledger: ledgerBox ? { left: ledgerBox.left, right: ledgerBox.right } : null,
          articles: articleResults,
        };
      });
      await page.close();

      const label = `homepage benchmark ${viewport.name}`;
      if (result.pageWidth > result.viewportWidth) {
        failures.push(`${label}: horizontal page overflow ${result.pageWidth}px > ${result.viewportWidth}px`);
      }
      if (!result.section || result.section.left < 0 || result.section.right > result.viewportWidth) {
        failures.push(`${label}: section escapes viewport`);
      }
      if (!result.ledger || result.ledger.left < 0 || result.ledger.right > result.viewportWidth) {
        failures.push(`${label}: ledger escapes viewport`);
      }
      for (const [index, item] of result.articles.entries()) {
        if (!item.value) failures.push(`${label}: metric ${index + 1} value is missing`);
        if (!item.detail) failures.push(`${label}: metric ${index + 1} detail is missing`);
        if (item.article.scrollWidth > item.article.clientWidth + 1) {
          failures.push(`${label}: metric ${index + 1} content overflows its cell`);
        }
        if (item.value && (item.value.left < item.article.left - 1 || item.value.right > item.article.right + 1)) {
          failures.push(`${label}: metric ${index + 1} value escapes its cell`);
        }
        if (item.detail && (item.detail.left < item.article.left - 1 || item.detail.right > item.article.right + 1)) {
          failures.push(`${label}: metric ${index + 1} detail escapes its cell`);
        }
        const sameRow = item.value && item.detail && item.value.bottom > item.detail.top && item.detail.bottom > item.value.top;
        if (sameRow && item.value.right > item.detail.left - 12) {
          failures.push(`${label}: metric ${index + 1} value crowds the detail text`);
        }
      }
    }
  } finally {
    await browser.close();
    server.close();
  }
}

await checkHomepageBenchmarkLayout();

if (failures.length) {
  console.error('Benchmark content check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log('Checked public benchmark route, homepage entry point, nav links, and omitted resource usage wording.');
