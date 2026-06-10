#!/usr/bin/env node

import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import sharp from "sharp";

const root = process.cwd();
const iconDir = path.join(root, "icons");
const outputDir = path.join(iconDir, "cutout");

const options = parseArgs(process.argv.slice(2));
const inputs = options.inputs.length > 0 ? options.inputs : await defaultInputs();

await fs.mkdir(outputDir, { recursive: true });

for (const input of inputs) {
  const inputPath = path.resolve(root, input);
  const parsed = path.parse(inputPath);
  const outputPath = options.output
    ? path.resolve(root, options.output)
    : path.join(outputDir, `${parsed.name}-cutout.png`);

  const stats = await cutoutIcon(inputPath, outputPath, options);
  console.log(
    `cutout ${path.relative(root, inputPath)} -> ${path.relative(root, outputPath)} ` +
      `(${stats.width}x${stats.height}, crop ${stats.crop.width}x${stats.crop.height})`,
  );
}

async function defaultInputs() {
  const files = await fs.readdir(iconDir, { withFileTypes: true });
  return files
    .filter((entry) => entry.isFile() && /\.png$/i.test(entry.name))
    .map((entry) => path.join("icons", entry.name));
}

async function cutoutIcon(inputPath, outputPath, opts) {
  const image = sharp(inputPath).ensureAlpha();
  const metadata = await image.metadata();
  const width = metadata.width;
  const height = metadata.height;

  if (!width || !height) {
    throw new Error(`Cannot read image size: ${inputPath}`);
  }

  const { data } = await image.raw().toBuffer({ resolveWithObject: true });
  const pixels = new Uint8ClampedArray(data.buffer, data.byteOffset, data.byteLength);
  const bg = sampleBackground(pixels, width, height);
  const removeMask = floodBackground(pixels, width, height, bg, opts);

  if (opts.removeWatermark) {
    clearWatermarkZone(removeMask, width, height, opts);
  }

  applyAlpha(pixels, removeMask, width, height, opts);
  const crop = findOpaqueBounds(pixels, width, height, opts.cropPadding);

  await sharp(Buffer.from(pixels), {
    raw: { width, height, channels: 4 },
  })
    .extract(crop)
    .png()
    .toFile(outputPath);

  return { width, height, crop };
}

function sampleBackground(pixels, width, height) {
  const marginX = Math.max(8, Math.floor(width * 0.035));
  const marginY = Math.max(8, Math.floor(height * 0.035));
  const samples = [];

  for (const [x0, y0] of [
    [marginX, marginY],
    [width - marginX - 1, marginY],
    [marginX, height - marginY - 1],
    [width - marginX - 1, height - marginY - 1],
  ]) {
    for (let y = y0 - 4; y <= y0 + 4; y++) {
      for (let x = x0 - 4; x <= x0 + 4; x++) {
        const offset = (clamp(y, 0, height - 1) * width + clamp(x, 0, width - 1)) * 4;
        samples.push([pixels[offset], pixels[offset + 1], pixels[offset + 2]]);
      }
    }
  }

  return medianRgb(samples);
}

function floodBackground(pixels, width, height, bg, opts) {
  const total = width * height;
  const removeMask = new Uint8Array(total);
  const queue = [];

  for (let x = 0; x < width; x++) {
    queue.push(x, (height - 1) * width + x);
  }
  for (let y = 1; y < height - 1; y++) {
    queue.push(y * width, y * width + width - 1);
  }

  for (let read = 0; read < queue.length; read++) {
    const index = queue[read];
    if (removeMask[index]) continue;

    const offset = index * 4;
    if (!isBackgroundLike(pixels, offset, bg, opts)) continue;

    removeMask[index] = 1;

    const x = index % width;
    const y = Math.floor(index / width);
    if (x > 0) queue.push(index - 1);
    if (x + 1 < width) queue.push(index + 1);
    if (y > 0) queue.push(index - width);
    if (y + 1 < height) queue.push(index + width);
  }

  return removeMask;
}

function isBackgroundLike(pixels, offset, bg, opts) {
  const r = pixels[offset];
  const g = pixels[offset + 1];
  const b = pixels[offset + 2];
  const saturation = max3(r, g, b) - min3(r, g, b);
  const distance = colorDistance([r, g, b], bg);

  return distance <= opts.tolerance || (saturation <= opts.grayTolerance && distance <= opts.looseTolerance);
}

function applyAlpha(pixels, removeMask, width, height, opts) {
  const expanded = dilate(removeMask, width, height, opts.feather);

  for (let i = 0; i < width * height; i++) {
    const offset = i * 4;
    if (removeMask[i]) {
      pixels[offset + 3] = 0;
    } else if (expanded[i]) {
      pixels[offset + 3] = Math.min(pixels[offset + 3], opts.edgeAlpha);
    }
  }
}

function dilate(mask, width, height, radius) {
  const out = new Uint8Array(mask.length);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const index = y * width + x;
      if (mask[index]) continue;

      let near = false;
      for (let dy = -radius; dy <= radius && !near; dy++) {
        const yy = y + dy;
        if (yy < 0 || yy >= height) continue;
        for (let dx = -radius; dx <= radius; dx++) {
          const xx = x + dx;
          if (xx < 0 || xx >= width) continue;
          if (mask[yy * width + xx]) {
            near = true;
            break;
          }
        }
      }
      out[index] = near ? 1 : 0;
    }
  }
  return out;
}

function clearWatermarkZone(mask, width, height, opts) {
  const startX = Math.floor(width * opts.watermarkX);
  const startY = Math.floor(height * opts.watermarkY);
  for (let y = startY; y < height; y++) {
    for (let x = startX; x < width; x++) {
      mask[y * width + x] = 1;
    }
  }
}

function findOpaqueBounds(pixels, width, height, padding) {
  let left = width;
  let top = height;
  let right = -1;
  let bottom = -1;

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const alpha = pixels[(y * width + x) * 4 + 3];
      if (alpha <= 8) continue;
      left = Math.min(left, x);
      top = Math.min(top, y);
      right = Math.max(right, x);
      bottom = Math.max(bottom, y);
    }
  }

  if (right < left || bottom < top) {
    return { left: 0, top: 0, width, height };
  }

  left = Math.max(0, left - padding);
  top = Math.max(0, top - padding);
  right = Math.min(width - 1, right + padding);
  bottom = Math.min(height - 1, bottom + padding);

  return { left, top, width: right - left + 1, height: bottom - top + 1 };
}

function parseArgs(args) {
  const opts = {
    inputs: [],
    output: "",
    tolerance: 38,
    looseTolerance: 62,
    grayTolerance: 14,
    feather: 2,
    edgeAlpha: 180,
    cropPadding: 18,
    removeWatermark: true,
    watermarkX: 0.78,
    watermarkY: 0.86,
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === "--out") opts.output = args[++i] ?? "";
    else if (arg === "--tolerance") opts.tolerance = Number(args[++i]);
    else if (arg === "--loose-tolerance") opts.looseTolerance = Number(args[++i]);
    else if (arg === "--gray-tolerance") opts.grayTolerance = Number(args[++i]);
    else if (arg === "--feather") opts.feather = Number(args[++i]);
    else if (arg === "--crop-padding") opts.cropPadding = Number(args[++i]);
    else if (arg === "--keep-watermark") opts.removeWatermark = false;
    else if (arg === "--watermark-x") opts.watermarkX = Number(args[++i]);
    else if (arg === "--watermark-y") opts.watermarkY = Number(args[++i]);
    else opts.inputs.push(arg);
  }

  return opts;
}

function medianRgb(samples) {
  return [0, 1, 2].map((channel) => {
    const values = samples.map((sample) => sample[channel]).sort((a, b) => a - b);
    return values[Math.floor(values.length / 2)];
  });
}

function colorDistance(a, b) {
  const dr = a[0] - b[0];
  const dg = a[1] - b[1];
  const db = a[2] - b[2];
  return Math.sqrt(dr * dr + dg * dg + db * db);
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function max3(a, b, c) {
  return Math.max(a, Math.max(b, c));
}

function min3(a, b, c) {
  return Math.min(a, Math.min(b, c));
}
