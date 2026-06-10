#!/usr/bin/env node

import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import sharp from "sharp";

const root = process.cwd();
const iconDir = path.join(root, "icons");
const outputDir = path.join(iconDir, "cropped");

const options = parseArgs(process.argv.slice(2));
const inputs = options.inputs.length > 0 ? options.inputs : await defaultInputs();

await fs.mkdir(outputDir, { recursive: true });

for (const input of inputs) {
  const inputPath = path.resolve(root, input);
  const parsed = path.parse(inputPath);
  const outputPath = options.output
    ? path.resolve(root, options.output)
    : path.join(outputDir, `${parsed.name}-cropped.png`);

  const result = await cropIcon(inputPath, outputPath, options);
  console.log(
    `crop ${path.relative(root, inputPath)} -> ${path.relative(root, outputPath)} ` +
      `(${result.original.width}x${result.original.height} -> ${result.crop.width}x${result.crop.height})`,
  );
}

async function defaultInputs() {
  const files = await fs.readdir(iconDir, { withFileTypes: true });
  return files
    .filter((entry) => entry.isFile() && /\.png$/i.test(entry.name))
    .map((entry) => path.join("icons", entry.name));
}

async function cropIcon(inputPath, outputPath, opts) {
  const image = sharp(inputPath);
  const metadata = await image.metadata();
  const width = metadata.width;
  const height = metadata.height;

  if (!width || !height) {
    throw new Error(`Cannot read image size: ${inputPath}`);
  }

  const crop = getCropBox(width, height, opts);
  let pipeline = image.extract(crop);

  if (opts.resize) {
    pipeline = pipeline.resize(opts.resize, opts.resize, {
      fit: "cover",
      kernel: sharp.kernel.lanczos3,
    });
  }

  await pipeline.png().toFile(outputPath);
  return { original: { width, height }, crop };
}

function getCropBox(width, height, opts) {
  const maxSquare = Math.min(width, height);
  const requestedSize = opts.size > 0 ? opts.size : Math.round(maxSquare * opts.sizeRatio);
  const cropSize = clamp(requestedSize, 1, maxSquare);

  const left = opts.left ?? Math.round((width - cropSize) * opts.centerX);
  const top = opts.top ?? Math.round(height * opts.topRatio);

  return {
    left: clamp(left, 0, width - cropSize),
    top: clamp(top, 0, height - cropSize),
    width: cropSize,
    height: cropSize,
  };
}

function parseArgs(args) {
  const opts = {
    inputs: [],
    output: "",
    size: 0,
    resize: 0,
    sizeRatio: 0.74,
    topRatio: 0.125,
    centerX: 0.5,
    left: null,
    top: null,
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === "--out") opts.output = args[++i] ?? "";
    else if (arg === "--size") opts.size = Number(args[++i]);
    else if (arg === "--resize") opts.resize = Number(args[++i]);
    else if (arg === "--size-ratio") opts.sizeRatio = Number(args[++i]);
    else if (arg === "--top-ratio") opts.topRatio = Number(args[++i]);
    else if (arg === "--center-x") opts.centerX = Number(args[++i]);
    else if (arg === "--left") opts.left = Number(args[++i]);
    else if (arg === "--top") opts.top = Number(args[++i]);
    else opts.inputs.push(arg);
  }

  return opts;
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}
