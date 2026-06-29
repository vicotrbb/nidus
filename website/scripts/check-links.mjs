#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const DIST = path.join(ROOT, 'website/dist');
const sitemap = JSON.parse(fs.readFileSync(path.join(DIST, 'site-map.json'), 'utf8'));
const base = sitemap.base === '/' ? '/' : sitemap.base;
const expectedDomain = (process.env.NIDUS_SITE_DOMAIN ?? '').trim();
const htmlFiles = [];

function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.name.endsWith('.html')) htmlFiles.push(full);
  }
}

function localPathFromHref(href) {
  if (href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:') || href.startsWith('#')) return null;
  const clean = href.split('#')[0].split('?')[0];
  if (!clean) return null;
  let route = clean;
  if (base !== '/' && route.startsWith(base)) route = route.slice(base.length - 1);
  if (route.startsWith('/')) route = route.slice(1);
  if (!route) return path.join(DIST, 'index.html');
  const target = path.join(DIST, route);
  if (path.extname(target)) return target;
  return path.join(target, 'index.html');
}

walk(DIST);
const broken = [];
const contextErrors = [];
const cname = path.join(DIST, 'CNAME');

if (expectedDomain) {
  if (!fs.existsSync(cname)) {
    contextErrors.push(`expected CNAME for ${expectedDomain}, but website/dist/CNAME is missing`);
  } else {
    const value = fs.readFileSync(cname, 'utf8').trim();
    if (value !== expectedDomain) contextErrors.push(`expected CNAME ${expectedDomain}, found ${value || '<empty>'}`);
  }
} else if (fs.existsSync(cname)) {
  contextErrors.push('website/dist/CNAME exists but NIDUS_SITE_DOMAIN is not set');
}

for (const file of htmlFiles) {
  const html = fs.readFileSync(file, 'utf8');
  for (const match of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
    const raw = match[1];
    if (base === '/' && raw.startsWith('/nidus/')) {
      contextErrors.push(`${path.relative(DIST, file)} -> stale project-base URL ${raw}`);
    }
    if (base !== '/' && raw.startsWith('/') && !raw.startsWith(base) && !raw.startsWith('//')) {
      contextErrors.push(`${path.relative(DIST, file)} -> root URL ${raw} does not include base ${base}`);
    }
    const target = localPathFromHref(raw);
    if (target && !fs.existsSync(target)) {
      broken.push(`${path.relative(DIST, file)} -> ${raw}`);
    }
  }
}

if (broken.length || contextErrors.length) {
  if (broken.length) console.error('Broken local links:');
  for (const item of broken) console.error(`- ${item}`);
  if (contextErrors.length) console.error('Deployment context errors:');
  for (const item of contextErrors) console.error(`- ${item}`);
  process.exit(1);
}

console.log(`Checked ${htmlFiles.length} HTML files; local links and deployment context ok.`);
