#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.resolve(__dirname, '../dist');
const sitemap = JSON.parse(fs.readFileSync(path.join(DIST, 'site-map.json'), 'utf8'));
const domain = sitemap.domain;
const failures = [];

if (!domain) {
  failures.push('site-map.json domain is empty; run the SEO check against the canonical domain build');
}

const origin = domain ? `https://${domain}` : '';
const expectedPages = sitemap.pages.map((page) => `/${page ? `${page}/` : ''}`);

function walk(dir, files = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, files);
    else if (entry.name.endsWith('.html')) files.push(full);
  }
  return files;
}

function attrsFor(html, tag, attrName, attrValue) {
  const pattern = new RegExp(`<${tag}[^>]*\\s${attrName}="${attrValue.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}"[^>]*>`, 'i');
  const match = pattern.exec(html);
  if (!match) return null;
  return Object.fromEntries([...match[0].matchAll(/([a-zA-Z:-]+)="([^"]*)"/g)].map((entry) => [entry[1], entry[2]]));
}

function metaContent(html, property) {
  return attrsFor(html, 'meta', 'property', property)?.content
    ?? attrsFor(html, 'meta', 'name', property)?.content
    ?? null;
}

function linkHref(html, rel) {
  return attrsFor(html, 'link', 'rel', rel)?.href ?? null;
}

function routeFromFile(file) {
  const relative = path.relative(DIST, file);
  if (relative === 'index.html') return '/';
  if (relative === '404.html') return '/404.html';
  return `/${relative.replace(/index\.html$/, '').replaceAll(path.sep, '/')}`;
}

function assertAbsoluteUrl(label, value) {
  if (!value) {
    failures.push(`${label} is missing`);
    return;
  }
  if (!value.startsWith(`${origin}/`)) failures.push(`${label} must be absolute on ${origin}, found ${value}`);
}

for (const file of walk(DIST)) {
  const html = fs.readFileSync(file, 'utf8');
  const route = routeFromFile(file);
  const canonicalRoute = route === '/404.html' ? '/' : route;
  const expectedUrl = `${origin}${canonicalRoute}`;
  const title = /<title>([^<]+)<\/title>/.exec(html)?.[1];
  const description = metaContent(html, 'description');

  if (!title || title.length < 12 || title.length > 70) failures.push(`${route}: title length should be 12-70 chars`);
  if (!description || description.length < 50 || description.length > 170) failures.push(`${route}: description length should be 50-170 chars`);
  if (linkHref(html, 'canonical') !== expectedUrl) failures.push(`${route}: canonical should be ${expectedUrl}`);
  if (!html.includes(`<link rel="alternate" hreflang="en" href="${expectedUrl}">`)) failures.push(`${route}: English hreflang alternate should be ${expectedUrl}`);
  if (!html.includes(`<link rel="alternate" hreflang="x-default" href="${expectedUrl}">`)) failures.push(`${route}: x-default hreflang alternate should be ${expectedUrl}`);
  if (metaContent(html, 'og:url') !== expectedUrl) failures.push(`${route}: og:url should be ${expectedUrl}`);
  if (metaContent(html, 'og:title') !== title) failures.push(`${route}: og:title should match <title>`);
  if (metaContent(html, 'og:description') !== description) failures.push(`${route}: og:description should match meta description`);
  if (metaContent(html, 'og:type') !== 'website') failures.push(`${route}: og:type should be website`);
  if (metaContent(html, 'og:site_name') !== 'Nidus') failures.push(`${route}: og:site_name should be Nidus`);
  if (metaContent(html, 'og:image:width') !== '1200') failures.push(`${route}: og:image:width should be 1200`);
  if (metaContent(html, 'og:image:height') !== '630') failures.push(`${route}: og:image:height should be 630`);
  if (metaContent(html, 'og:image:alt') !== 'Nidus Rust backend framework') failures.push(`${route}: og:image:alt is missing or incorrect`);
  assertAbsoluteUrl(`${route}: og:image`, metaContent(html, 'og:image'));
  if (metaContent(html, 'twitter:card') !== 'summary_large_image') failures.push(`${route}: twitter:card should be summary_large_image`);
  if (metaContent(html, 'twitter:title') !== title) failures.push(`${route}: twitter:title should match <title>`);
  if (metaContent(html, 'twitter:description') !== description) failures.push(`${route}: twitter:description should match meta description`);
  assertAbsoluteUrl(`${route}: twitter:image`, metaContent(html, 'twitter:image'));
  if (!html.includes('<script type="application/ld+json">')) failures.push(`${route}: JSON-LD structured data is missing`);
}

const robotsPath = path.join(DIST, 'robots.txt');
if (!fs.existsSync(robotsPath)) {
  failures.push('robots.txt is missing');
} else {
  const robots = fs.readFileSync(robotsPath, 'utf8');
  if (!robots.includes(`Sitemap: ${origin}/sitemap.xml`)) failures.push('robots.txt must point to the canonical sitemap.xml');
}

const sitemapPath = path.join(DIST, 'sitemap.xml');
if (!fs.existsSync(sitemapPath)) {
  failures.push('sitemap.xml is missing');
} else {
  const xml = fs.readFileSync(sitemapPath, 'utf8');
  for (const page of expectedPages) {
    const loc = `<loc>${origin}${page}</loc>`;
    if (!xml.includes(loc)) failures.push(`sitemap.xml is missing ${loc}`);
  }
}

if (failures.length) {
  console.error('SEO check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Checked SEO metadata for ${expectedPages.length} canonical pages on ${origin}.`);
