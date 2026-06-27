#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const DIST = path.join(ROOT, 'website/dist');
const sitemap = JSON.parse(fs.readFileSync(path.join(DIST, 'site-map.json'), 'utf8'));
const base = sitemap.base === '/' ? '/' : sitemap.base;
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
for (const file of htmlFiles) {
  const html = fs.readFileSync(file, 'utf8');
  for (const match of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
    const target = localPathFromHref(match[1]);
    if (target && !fs.existsSync(target)) {
      broken.push(`${path.relative(DIST, file)} -> ${match[1]}`);
    }
  }
}

if (broken.length) {
  console.error('Broken local links:');
  for (const item of broken) console.error(`- ${item}`);
  process.exit(1);
}

console.log(`Checked ${htmlFiles.length} HTML files; local links ok.`);
