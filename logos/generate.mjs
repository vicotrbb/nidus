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
const VIOLET_DARK = [16, 9, 32, 255];
const VIOLET_DEEP = [28, 12, 58, 255];
const VIOLET_MID = [112, 42, 230, 255];
const VIOLET_HOT = [190, 93, 255, 255];
const TEXT_BRIGHT = [247, 241, 255, 255];
const TEXT_MUTED = [190, 171, 224, 255];

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

  const inner = 84;
  const outer = 176;
  for (let i = 0; i < out.data.length; i += 4) {
    const r = out.data[i];
    const g = out.data[i + 1];
    const b = out.data[i + 2];
    const greenDominance = g - Math.max(r, b);
    const dist = Math.hypot(r - kr, g - kg, b - kb);
    const isGreenScreen = g > 64 && greenDominance > 22 && g > r * 1.08 && g > b * 1.08;
    const isEdgeSpill = g > 80 && greenDominance > 12 && dist < outer * 1.25;

    if (dist < inner || isGreenScreen || isEdgeSpill) {
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

function softenTransparentEdges(image) {
  const out = cloneImage(image);
  for (let y = 1; y < image.height - 1; y++) {
    for (let x = 1; x < image.width - 1; x++) {
      const i = (y * image.width + x) * 4;
      const a = image.data[i + 3];
      if (a === 0 || a === 255) continue;

      let neighborAlpha = 0;
      for (let oy = -1; oy <= 1; oy++) {
        for (let ox = -1; ox <= 1; ox++) {
          neighborAlpha += image.data[((y + oy) * image.width + x + ox) * 4 + 3];
        }
      }
      const coverage = neighborAlpha / (255 * 9);
      out.data[i + 3] = Math.round(a * Math.max(0.35, coverage));
      out.data[i + 1] = Math.min(out.data[i + 1], Math.max(out.data[i], out.data[i + 2]) + 12);
    }
  }
  return out;
}

function removeTinyAlphaIslands(image, minPixels = 22) {
  const out = cloneImage(image);
  const visited = new Uint8Array(image.width * image.height);
  const stack = [];
  const component = [];

  for (let y = 0; y < image.height; y++) {
    for (let x = 0; x < image.width; x++) {
      const start = y * image.width + x;
      if (visited[start] || image.data[start * 4 + 3] <= 10) continue;

      let count = 0;
      stack.length = 0;
      component.length = 0;
      stack.push(start);
      visited[start] = 1;

      while (stack.length) {
        const index = stack.pop();
        component.push(index);
        count++;
        const px = index % image.width;
        const py = Math.floor(index / image.width);
        const neighbors = [
          index - 1,
          index + 1,
          index - image.width,
          index + image.width,
        ];

        for (const next of neighbors) {
          if (next < 0 || next >= visited.length || visited[next]) continue;
          const nx = next % image.width;
          const ny = Math.floor(next / image.width);
          if (Math.abs(nx - px) + Math.abs(ny - py) !== 1) continue;
          if (image.data[next * 4 + 3] <= 10) continue;
          visited[next] = 1;
          stack.push(next);
        }
      }

      if (count < minPixels) {
        for (const index of component) {
          const i = index * 4;
          out.data[i] = 0;
          out.data[i + 1] = 0;
          out.data[i + 2] = 0;
          out.data[i + 3] = 0;
        }
      }
    }
  }

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

function resizeCover(image, width, height) {
  const scale = Math.max(width / image.width, height / image.height);
  const w = Math.max(1, Math.round(image.width * scale));
  const h = Math.max(1, Math.round(image.height * scale));
  const resized = resize(image, w, h);
  return crop(resized, Math.floor((w - width) / 2), Math.floor((h - height) / 2), width, height);
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

function clamp(value, min = 0, max = 255) {
  return Math.max(min, Math.min(max, value));
}

function mix(a, b, t) {
  return Math.round(a + (b - a) * t);
}

function blendPixel(image, x, y, color, alpha = 1) {
  if (x < 0 || y < 0 || x >= image.width || y >= image.height) return;
  const i = (y * image.width + x) * 4;
  const a = clamp((color[3] / 255) * alpha, 0, 1);
  const inv = 1 - a;
  image.data[i] = Math.round(color[0] * a + image.data[i] * inv);
  image.data[i + 1] = Math.round(color[1] * a + image.data[i + 1] * inv);
  image.data[i + 2] = Math.round(color[2] * a + image.data[i + 2] * inv);
  image.data[i + 3] = Math.round(255 * (a + (image.data[i + 3] / 255) * inv));
}

function gradient(width, height) {
  const out = solid(width, height, VIOLET_DARK);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      const nx = x / (width - 1);
      const ny = y / (height - 1);
      const leftGlow = Math.max(0, 1 - Math.hypot((nx - 0.25) / 0.58, (ny - 0.38) / 0.74));
      const rightGlow = Math.max(0, 1 - Math.hypot((nx - 0.86) / 0.42, (ny - 0.2) / 0.52));
      const base = 0.22 + leftGlow * 0.62 + rightGlow * 0.34 + (1 - ny) * 0.08;
      out.data[i] = mix(VIOLET_DARK[0], VIOLET_DEEP[0], base);
      out.data[i + 1] = mix(VIOLET_DARK[1], VIOLET_DEEP[1], base);
      out.data[i + 2] = mix(VIOLET_DARK[2], VIOLET_DEEP[2], base);
      out.data[i + 3] = 255;
    }
  }
  return out;
}

function drawRadialGlow(image, cx, cy, radius, color, strength = 1) {
  const minX = Math.max(0, Math.floor(cx - radius));
  const maxX = Math.min(image.width - 1, Math.ceil(cx + radius));
  const minY = Math.max(0, Math.floor(cy - radius));
  const maxY = Math.min(image.height - 1, Math.ceil(cy + radius));
  for (let y = minY; y <= maxY; y++) {
    for (let x = minX; x <= maxX; x++) {
      const d = Math.hypot(x - cx, y - cy) / radius;
      if (d > 1) continue;
      blendPixel(image, x, y, color, (1 - d) * (1 - d) * strength);
    }
  }
}

function drawRing(image, cx, cy, radius, thickness, color, alpha = 1, start = 0, end = Math.PI * 2) {
  const minX = Math.max(0, Math.floor(cx - radius - thickness));
  const maxX = Math.min(image.width - 1, Math.ceil(cx + radius + thickness));
  const minY = Math.max(0, Math.floor(cy - radius - thickness));
  const maxY = Math.min(image.height - 1, Math.ceil(cy + radius + thickness));
  for (let y = minY; y <= maxY; y++) {
    for (let x = minX; x <= maxX; x++) {
      const angle = Math.atan2(y - cy, x - cx);
      const normalized = angle < 0 ? angle + Math.PI * 2 : angle;
      const inArc = start <= end ? normalized >= start && normalized <= end : normalized >= start || normalized <= end;
      if (!inArc) continue;
      const d = Math.abs(Math.hypot(x - cx, y - cy) - radius);
      if (d > thickness) continue;
      blendPixel(image, x, y, color, (1 - d / thickness) * alpha);
    }
  }
}

function drawRoundedRect(image, left, top, width, height, radius, color, alpha = 1) {
  for (let y = top; y < top + height; y++) {
    for (let x = left; x < left + width; x++) {
      const dx = x < left + radius ? left + radius - x : x >= left + width - radius ? x - (left + width - radius - 1) : 0;
      const dy = y < top + radius ? top + radius - y : y >= top + height - radius ? y - (top + height - radius - 1) : 0;
      if (Math.hypot(dx, dy) <= radius) blendPixel(image, x, y, color, alpha);
    }
  }
}

function drawLine(image, x1, y1, x2, y2, color, alpha = 1, thickness = 1) {
  const steps = Math.max(Math.abs(x2 - x1), Math.abs(y2 - y1));
  for (let s = 0; s <= steps; s++) {
    const t = steps === 0 ? 0 : s / steps;
    const x = Math.round(x1 + (x2 - x1) * t);
    const y = Math.round(y1 + (y2 - y1) * t);
    for (let oy = -thickness; oy <= thickness; oy++) {
      for (let ox = -thickness; ox <= thickness; ox++) {
        if (Math.hypot(ox, oy) <= thickness) blendPixel(image, x + ox, y + oy, color, alpha);
      }
    }
  }
}

function drawTextBars(image, left, top, widths, color, alpha = 1) {
  widths.forEach((width, index) => {
    drawRoundedRect(image, left, top + index * 32, width, 11, 4, color, alpha);
  });
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
  const transparent = trim(removeTinyAlphaIslands(softenTransparentEdges(chromaKey(source))), 10);
  const mark = extractMark(transparent);
  const squareMark = resizeContain(mark, Math.max(mark.width, mark.height), Math.max(mark.width, mark.height));

  write('logo-full-transparent.png', transparent);
  write('logo-mark-transparent.png', mark);
  write('logo-mark-square-transparent.png', squareMark);

  for (const size of FAVICON_SIZES) {
    const name = size === 180 ? 'apple-touch-icon.png' : size >= 192 ? `icon-${size}.png` : `favicon-${size}.png`;
    write(name, resizeContain(squareMark, size, size, [0, 0, 0, 0], 0.84));
  }

  for (const size of BRANDED_SIZES) {
    write(`favicon-branded-${size}.png`, resizeContain(squareMark, size, size, [24, 10, 50, 255], 0.76));
  }

  write('logo-on-light.png', resizeContain(transparent, 1024, 1024, [247, 244, 255, 255], 0.9));
  write('logo-on-dark.png', resizeContain(transparent, 1024, 1024, [18, 12, 34, 255], 0.9));
  write('site-logo-light.png', resizeContain(transparent, 960, 320, [0, 0, 0, 0], 0.86));
  write('site-logo-dark.png', resizeContain(transparent, 960, 320, [0, 0, 0, 0], 0.86));

  const og = gradient(1200, 630);
  drawRadialGlow(og, 340, 330, 420, [116, 45, 255, 255], 0.5);
  drawRadialGlow(og, 975, 95, 280, [189, 94, 255, 255], 0.22);
  drawRing(og, 376, 326, 260, 2.5, VIOLET_HOT, 0.44, 3.65, 6.05);
  drawRing(og, 376, 326, 315, 1.8, VIOLET_MID, 0.36, 3.78, 0.32);
  drawRing(og, 376, 326, 374, 1.5, VIOLET_HOT, 0.22, 3.96, 0.12);
  drawLine(og, 760, 145, 1070, 145, [119, 48, 233, 255], 0.34, 1);
  drawLine(og, 760, 493, 1088, 493, [119, 48, 233, 255], 0.3, 1);
  drawRoundedRect(og, 692, 178, 420, 274, 18, [11, 7, 24, 255], 0.66);
  drawRoundedRect(og, 711, 199, 382, 44, 10, [35, 18, 70, 255], 0.9);
  drawRoundedRect(og, 734, 217, 15, 15, 7, [255, 95, 132, 255], 0.8);
  drawRoundedRect(og, 760, 217, 15, 15, 7, [255, 196, 87, 255], 0.78);
  drawRoundedRect(og, 786, 217, 15, 15, 7, [81, 230, 154, 255], 0.76);
  drawTextBars(og, 730, 276, [292, 238, 326, 198], TEXT_MUTED, 0.5);
  drawTextBars(og, 730, 290, [180, 318, 252, 284], VIOLET_HOT, 0.18);
  drawRoundedRect(og, 728, 408, 206, 14, 6, TEXT_BRIGHT, 0.72);
  const ogGhost = resizeCover(mark, 1120, 1120);
  for (let i = 3; i < ogGhost.data.length; i += 4) ogGhost.data[i] = Math.round(ogGhost.data[i] * 0.08);
  composite(og, ogGhost, -170, -245);
  const ogMark = resizeContain(mark, 570, 570, [0, 0, 0, 0], 0.94);
  composite(og, ogMark, 76, 29);
  write('og-image.png', og);
}

main();
