#!/usr/bin/env node
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.resolve(__dirname, '../dist');
const playwrightSpecifier = process.env.PLAYWRIGHT_MODULE_PATH ?? 'playwright';
const siteMap = JSON.parse(fs.readFileSync(path.join(DIST, 'site-map.json'), 'utf8'));

function contentType(file) {
  if (file.endsWith('.html')) return 'text/html; charset=utf-8';
  if (file.endsWith('.css')) return 'text/css; charset=utf-8';
  if (file.endsWith('.js')) return 'text/javascript; charset=utf-8';
  if (file.endsWith('.png')) return 'image/png';
  if (file.endsWith('.json')) return 'application/json; charset=utf-8';
  return 'application/octet-stream';
}

function serveDist() {
  const server = http.createServer((request, response) => {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1');
    const pathname = decodeURIComponent(url.pathname);
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

function isTransparent(value) {
  return value === 'rgba(0, 0, 0, 0)' || value === 'transparent';
}

const viewports = [
  { name: 'narrow phone', width: 320, height: 568 },
  { name: 'iPhone portrait', width: 390, height: 844 },
  { name: 'reported viewport', width: 402, height: 874 },
  { name: 'large phone', width: 430, height: 932 },
];

const failures = [];
const { chromium } = await import(playwrightSpecifier);
const { server, url } = await serveDist();
const browser = await chromium.launch({ headless: true });

try {
  for (const viewport of viewports) {
    const page = await browser.newPage({
      viewport: { width: viewport.width, height: viewport.height },
      deviceScaleFactor: 3,
      isMobile: true,
      hasTouch: true,
    });
    await page.goto(url, { waitUntil: 'networkidle' });
    const result = await page.evaluate(() => {
      const rect = (selector) => {
        const element = document.querySelector(selector);
        if (!element) return null;
        const box = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return {
          x: box.x,
          y: box.y,
          width: box.width,
          height: box.height,
          right: box.right,
          bottom: box.bottom,
          display: style.display,
          borderTopColor: style.borderTopColor,
          backgroundColor: style.backgroundColor,
        };
      };
      return {
        viewport: { width: innerWidth, height: innerHeight },
        scrollWidth: document.documentElement.scrollWidth,
        header: rect('.site-header'),
        mobileMark: rect('.mobile-hero-mark'),
        desktopHeroMark: rect('.hero-proof > img'),
        actions: rect('.hero-actions'),
        buttons: [...document.querySelectorAll('.hero-actions .button')].map((button) => {
          const box = button.getBoundingClientRect();
          const style = getComputedStyle(button);
          return {
            text: button.textContent.trim(),
            width: box.width,
            height: box.height,
            borderTopColor: style.borderTopColor,
            backgroundColor: style.backgroundColor,
          };
        }),
      };
    });

    const label = `${viewport.name} ${viewport.width}x${viewport.height}`;
    if (result.scrollWidth > result.viewport.width) {
      failures.push(`${label}: horizontal overflow ${result.scrollWidth}px > ${result.viewport.width}px`);
    }
    if (!result.header || result.header.x < 0 || result.header.right > result.viewport.width) {
      failures.push(`${label}: header escapes viewport`);
    }
    if (!result.mobileMark || result.mobileMark.display === 'none') {
      failures.push(`${label}: mobile hero mark is missing`);
    } else {
      if (result.mobileMark.x < 0 || result.mobileMark.right > result.viewport.width) {
        failures.push(`${label}: mobile hero mark escapes horizontally`);
      }
      if (result.mobileMark.y < 0 || result.mobileMark.bottom > result.viewport.height - 16) {
        failures.push(`${label}: mobile hero mark is clipped in the first viewport`);
      }
    }
    if (result.desktopHeroMark && result.desktopHeroMark.display !== 'none') {
      failures.push(`${label}: desktop hero art is still visible on phone layout`);
    }
    if (!result.actions || result.actions.height > 126) {
      failures.push(`${label}: hero actions are too tall for mobile (${result.actions?.height ?? 'missing'}px)`);
    }
    for (const button of result.buttons) {
      if (button.height < 44) failures.push(`${label}: ${button.text} touch target is shorter than 44px`);
      if (isTransparent(button.borderTopColor) && isTransparent(button.backgroundColor)) {
        failures.push(`${label}: ${button.text} has no visible mobile button affordance`);
      }
    }

    for (const route of siteMap.pages) {
      const routePath = route ? `${route}/` : '';
      await page.goto(new URL(routePath, url).href, { waitUntil: 'networkidle' });
      const pageResult = await page.evaluate(() => {
        const header = document.querySelector('.site-header')?.getBoundingClientRect();
        return {
          pathname: location.pathname,
          viewportWidth: innerWidth,
          scrollWidth: document.documentElement.scrollWidth,
          header: header
            ? { x: header.x, right: header.right, width: header.width }
            : null,
        };
      });
      const routeLabel = `${label} ${pageResult.pathname}`;
      if (pageResult.scrollWidth > pageResult.viewportWidth) {
        failures.push(`${routeLabel}: horizontal overflow ${pageResult.scrollWidth}px > ${pageResult.viewportWidth}px`);
      }
      if (!pageResult.header || pageResult.header.x < 0 || pageResult.header.right > pageResult.viewportWidth) {
        failures.push(`${routeLabel}: header escapes viewport`);
      }
    }
    await page.close();
  }
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error('Mobile layout check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Checked ${viewports.length} mobile viewport layouts and ${siteMap.pages.length} generated routes; no clipping, overflow, or CTA regressions.`);
