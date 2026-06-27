#!/usr/bin/env node
/**
 * Nidus logo asset generator.
 *
 * Source: logos/nidus-logo-with-bg.png
 *
 * The source image includes a green background. This script removes that
 * background with a deterministic chroma key and writes transparent, favicon,
 * social, and website-ready variants without external npm dependencies.
 */
import fs from 'fs';
import path from 'path';
import zlib from 'zlib';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.join(__dirname, 'nidus-logo-with-bg.png');
const OUT = __dirname;

const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const FAVICON_SIZES = [16, 32, 48, 96, 180, 192, 512];
const BRANDED_SIZES = [16, 32, 48, 96, 180, 192, 512];

const crcTable = new Uint32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) {
    c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  crcTable[n] = c >>> 0;
}

function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data = Buffer.alloc(0)) {
  const typeBuf = Buffer.from(type, 'ascii');
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  typeBuf.copy(out, 4);
  data.copy(out, 8);
  out.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 8 + data.length);
  return out;
}

function readPng(file) {
  const bytes = fs.readFileSync(file);
  if (!bytes.subarray(0, 8).equals(PNG_SIGNATURE)) {
    throw new Error(`${file} is not a PNG`);
  }

  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const idat = [];

  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.subarray(offset + 4, offset + 8).toString('ascii');
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;

    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      const bitDepth = data[8];
      colorType = data[9];
      const interlace = data[12];
      if (bitDepth !== 8 || interlace !== 0 || (colorType !== 2 && colorType !== 6)) {
        throw new Error('Only non-interlaced 8-bit RGB/RGBA PNG sources are supported');
      }
    } else if (type === 'IDAT') {
      idat.push(data);
    } else if (type === 'IEND') {
      break;
    }
  }

  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = zlib.inflateSync(Buffer.concat(idat));
  const rgba = new Uint8Array(width * height * 4);
  const prev = new Uint8Array(stride);
  const row = new Uint8Array(stride);
  let pos = 0;

  for (let y = 0; y < height; y++) {
    const filter = inflated[pos++];
    for (let x = 0; x < stride; x++) {
      const raw = inflated[pos++];
      const left = x >= channels ? row[x - channels] : 0;
      const up = prev[x];
      const upLeft = x >= channels ? prev[x - channels] : 0;
      if (filter === 0) row[x] = raw;
      else if (filter === 1) row[x] = (raw + left) & 0xff;
      else if (filter === 2) row[x] = (raw + up) & 0xff;
      else if (filter === 3) row[x] = (raw + Math.floor((left + up) / 2)) & 0xff;
      else if (filter === 4) row[x] = (raw + paeth(left, up, upLeft)) & 0xff;
      else throw new Error(`Unsupported PNG filter ${filter}`);
    }

    for (let x = 0; x < width; x++) {
      const src = x * channels;
      const dst = (y * width + x) * 4;
      rgba[dst] = row[src];
      rgba[dst + 1] = row[src + 1];
      rgba[dst + 2] = row[src + 2];
      rgba[dst + 3] = channels === 4 ? row[src + 3] : 255;
    }
    prev.set(row);
  }

  return { width, height, data: rgba };
}

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

function writePng(file, image) {
  const { width, height, data } = image;
  const scanline = width * 4 + 1;
  const raw = Buffer.alloc(scanline * height);
  for (let y = 0; y < height; y++) {
    const row = y * scanline;
    raw[row] = 0;
    Buffer.from(data.buffer, data.byteOffset + y * width * 4, width * 4).copy(raw, row + 1);
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const bytes = Buffer.concat([
    PNG_SIGNATURE,
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND'),
  ]);
  fs.writeFileSync(file, bytes);
}

function sampleKey(image) {
  const points = [
    [0, 0],
    [image.width - 1, 0],
    [0, image.height - 1],
    [image.width - 1, image.height - 1],
  ];
  const sum = [0, 0, 0];
  for (const [x, y] of points) {
    const i = (y * image.width + x) * 4;
    sum[0] += image.data[i];
    sum[1] += image.data[i + 1];
    sum[2] += image.data[i + 2];
  }
  return sum.map((v) => Math.round(v / points.length));
}

function chromaKey(image) {
  const [kr, kg, kb] = sampleKey(image);
  const out = cloneImage(image);
  let removed = 0;

  const inner = 74;
  const outer = 154;
  for (let i = 0; i < out.data.length; i += 4) {
    const r = out.data[i];
    const g = out.data[i + 1];
    const b = out.data[i + 2];
    const greenDominance = g - Math.max(r, b);
    const dist = Math.hypot(r - kr, g - kg, b - kb);
    const isGreenScreen = g > 70 && greenDominance > 28 && g > r * 1.12 && g > b * 1.12;

    if (dist < inner || isGreenScreen) {
      out.data[i] = 0;
      out.data[i + 1] = 0;
      out.data[i + 2] = 0;
      out.data[i + 3] = 0;
      removed++;
    } else if (dist < outer) {
      out.data[i + 3] = Math.round(((dist - inner) / (outer - inner)) * 255);
      out.data[i + 1] = Math.min(g, Math.max(r, b));
    } else if (g > r * 1.35 && g > b * 1.35) {
      out.data[i + 1] = Math.min(g, Math.max(r, b));
    }
  }

  console.log(`key rgb(${kr}, ${kg}, ${kb}); removed ${removed.toLocaleString()} pixels`);
  return out;
}

function cloneImage(image) {
  return { width: image.width, height: image.height, data: new Uint8Array(image.data) };
}

function crop(image, left, top, width, height) {
  const out = { width, height, data: new Uint8Array(width * height * 4) };
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const sx = left + x;
      const sy = top + y;
      if (sx < 0 || sy < 0 || sx >= image.width || sy >= image.height) continue;
      out.data.set(image.data.subarray((sy * image.width + sx) * 4, (sy * image.width + sx) * 4 + 4), (y * width + x) * 4);
    }
  }
  return out;
}

function trim(image, pad = 0) {
  let minX = image.width;
  let minY = image.height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < image.height; y++) {
    for (let x = 0; x < image.width; x++) {
      if (image.data[(y * image.width + x) * 4 + 3] > 8) {
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
    }
  }
  if (maxX < minX) return cloneImage(image);
  minX = Math.max(0, minX - pad);
  minY = Math.max(0, minY - pad);
  maxX = Math.min(image.width - 1, maxX + pad);
  maxY = Math.min(image.height - 1, maxY + pad);
  return crop(image, minX, minY, maxX - minX + 1, maxY - minY + 1);
}

function extractMark(full) {
  // The source image is already a standalone mark, not a horizontal lockup.
  // Preserve the full symbol; do not infer a horizontal split from internal
  // negative space, or the lower pods get cropped off.
  return trim(full, 4);
}

function resizeContain(image, width, height, background = [0, 0, 0, 0], scale = 1) {
  const targetW = Math.max(1, Math.round(image.width * scale));
  const targetH = Math.max(1, Math.round(image.height * scale));
  const actualScale = Math.min(width / targetW, height / targetH);
  const w = Math.max(1, Math.round(targetW * actualScale));
  const h = Math.max(1, Math.round(targetH * actualScale));
  const resized = resize(image, w, h);
  const out = solid(width, height, background);
  composite(out, resized, Math.floor((width - w) / 2), Math.floor((height - h) / 2));
  return out;
}

function resize(image, width, height) {
  const out = { width, height, data: new Uint8Array(width * height * 4) };
  const xRatio = image.width / width;
  const yRatio = image.height / height;
  for (let y = 0; y < height; y++) {
    const sy = (y + 0.5) * yRatio - 0.5;
    const y0 = Math.max(0, Math.floor(sy));
    const y1 = Math.min(image.height - 1, y0 + 1);
    const wy = sy - y0;
    for (let x = 0; x < width; x++) {
      const sx = (x + 0.5) * xRatio - 0.5;
      const x0 = Math.max(0, Math.floor(sx));
      const x1 = Math.min(image.width - 1, x0 + 1);
      const wx = sx - x0;
      const dst = (y * width + x) * 4;
      for (let c = 0; c < 4; c++) {
        const p00 = image.data[(y0 * image.width + x0) * 4 + c];
        const p10 = image.data[(y0 * image.width + x1) * 4 + c];
        const p01 = image.data[(y1 * image.width + x0) * 4 + c];
        const p11 = image.data[(y1 * image.width + x1) * 4 + c];
        out.data[dst + c] = Math.round(
          p00 * (1 - wx) * (1 - wy) + p10 * wx * (1 - wy) + p01 * (1 - wx) * wy + p11 * wx * wy,
        );
      }
    }
  }
  return out;
}

function solid(width, height, color) {
  const out = { width, height, data: new Uint8Array(width * height * 4) };
  for (let i = 0; i < out.data.length; i += 4) {
    out.data[i] = color[0];
    out.data[i + 1] = color[1];
    out.data[i + 2] = color[2];
    out.data[i + 3] = color[3];
  }
  return out;
}

function composite(base, overlay, left, top) {
  for (let y = 0; y < overlay.height; y++) {
    for (let x = 0; x < overlay.width; x++) {
      const bx = left + x;
      const by = top + y;
      if (bx < 0 || by < 0 || bx >= base.width || by >= base.height) continue;
      const src = (y * overlay.width + x) * 4;
      const dst = (by * base.width + bx) * 4;
      const a = overlay.data[src + 3] / 255;
      const inv = 1 - a;
      base.data[dst] = Math.round(overlay.data[src] * a + base.data[dst] * inv);
      base.data[dst + 1] = Math.round(overlay.data[src + 1] * a + base.data[dst + 1] * inv);
      base.data[dst + 2] = Math.round(overlay.data[src + 2] * a + base.data[dst + 2] * inv);
      base.data[dst + 3] = Math.round(255 * (a + (base.data[dst + 3] / 255) * inv));
    }
  }
}

function write(name, image) {
  writePng(path.join(OUT, name), image);
  console.log(`${name} ${image.width}x${image.height}`);
}

function main() {
  const source = readPng(SRC);
  const transparent = trim(chromaKey(source), 4);
  const mark = extractMark(transparent);
  const squareMark = resizeContain(mark, Math.max(mark.width, mark.height), Math.max(mark.width, mark.height));

  write('logo-full-transparent.png', transparent);
  write('logo-mark-transparent.png', mark);
  write('logo-mark-square-transparent.png', squareMark);

  for (const size of FAVICON_SIZES) {
    const name = size === 180 ? 'apple-touch-icon.png' : size >= 192 ? `icon-${size}.png` : `favicon-${size}.png`;
    write(name, resizeContain(squareMark, size, size, [0, 0, 0, 0], 0.88));
  }

  for (const size of BRANDED_SIZES) {
    write(`favicon-branded-${size}.png`, resizeContain(squareMark, size, size, [34, 16, 72, 255], 0.72));
  }

  write('logo-on-light.png', resizeContain(transparent, 1024, 1024, [247, 244, 255, 255], 0.9));
  write('logo-on-dark.png', resizeContain(transparent, 1024, 1024, [18, 12, 34, 255], 0.9));
  write('site-logo-light.png', resizeContain(transparent, 960, 320, [0, 0, 0, 0], 0.92));
  write('site-logo-dark.png', resizeContain(transparent, 960, 320, [0, 0, 0, 0], 0.92));

  const og = solid(1200, 630, [18, 12, 34, 255]);
  const ogMark = resizeContain(mark, 520, 520, [0, 0, 0, 0], 0.9);
  const ogLogo = resizeContain(transparent, 560, 250, [0, 0, 0, 0], 0.95);
  composite(og, ogMark, 70, 55);
  composite(og, ogLogo, 560, 120);
  write('og-image.png', og);
}

main();
