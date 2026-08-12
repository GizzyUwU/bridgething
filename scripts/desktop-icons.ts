#!/usr/bin/env bun
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = join(import.meta.dir, '..');
const SOURCE = join(ROOT, 'site/public/brand/icon-dark.svg');
const ICONS = join(ROOT, 'desktop/src-tauri/icons');
const WORK = join(ROOT, 'target/desktop-icons');

const TILE = '#0a0c0e';
const CANVAS = 1024;
const INSET = 100;
const RADIUS = 185.4;
const GLYPH = { width: 859.1, height: 538.25, fill: 0.7 };

const TRAY = 36;
const TRAY_LINUX = 64;

async function run(command: string[]): Promise<void> {
  const proc = Bun.spawn(command, { cwd: ROOT, stdout: 'pipe', stderr: 'pipe' });
  const code = await proc.exited;
  if (code !== 0) {
    console.error(await new Response(proc.stderr).text());
    throw new Error(`${command[0]} failed with ${code}`);
  }
}

function glyph(): string {
  const raw = readFileSync(SOURCE, 'utf8');
  const inner = raw.slice(raw.indexOf('>', raw.indexOf('<svg')) + 1, raw.lastIndexOf('</svg>'));
  return inner.replace(/\s*filter="url\([^)]*\)"/g, '');
}

function placed(body: string, box: number): string {
  const width = box * GLYPH.fill;
  const scale = width / GLYPH.width;
  const x = (CANVAS - width) / 2;
  const y = (CANVAS - GLYPH.height * scale) / 2;
  return `<g transform="translate(${x.toFixed(3)} ${y.toFixed(3)}) scale(${scale.toFixed(6)})">${body}</g>`;
}

function tile(): string {
  const plate = `<rect x="${INSET}" y="${INSET}" width="${CANVAS - INSET * 2}" height="${CANVAS - INSET * 2}" rx="${RADIUS}" fill="${TILE}"/>`;
  return svg(CANVAS, CANVAS, plate + placed(glyph(), CANVAS - INSET * 2));
}

function template(): string {
  const body = glyph().replace(/white|#EFEFEF|#00A8E8/g, '#000000');
  const y = (GLYPH.width - GLYPH.height) / 2;
  return svg(GLYPH.width, GLYPH.width, `<g transform="translate(0 ${y.toFixed(3)})">${body}</g>`);
}

function svg(width: number, height: number, body: string): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" fill="none">${body}</svg>`;
}

async function rasterize(source: string, target: string, width: number): Promise<void> {
  await run(['inkscape', source, '--export-type=png', `--export-filename=${target}`, `--export-width=${width}`]);
}

async function icns(master: string): Promise<void> {
  const set = join(WORK, 'icon.iconset');
  mkdirSync(set, { recursive: true });
  const sizes: [number, string][] = [
    [16, 'icon_16x16.png'],
    [32, 'icon_16x16@2x.png'],
    [32, 'icon_32x32.png'],
    [64, 'icon_32x32@2x.png'],
    [128, 'icon_128x128.png'],
    [256, 'icon_128x128@2x.png'],
    [256, 'icon_256x256.png'],
    [512, 'icon_256x256@2x.png'],
    [512, 'icon_512x512.png'],
    [1024, 'icon_512x512@2x.png'],
  ];
  for (const [size, name] of sizes) {
    await run(['sips', '-z', String(size), String(size), master, '--out', join(set, name)]);
  }
  await run(['iconutil', '-c', 'icns', set, '-o', join(ICONS, 'icon.icns')]);
}

rmSync(WORK, { recursive: true, force: true });
mkdirSync(WORK, { recursive: true });
mkdirSync(ICONS, { recursive: true });

const tileSvg = join(WORK, 'tile.svg');
const traySvg = join(WORK, 'tray.svg');
writeFileSync(tileSvg, tile());
writeFileSync(traySvg, template());

const master = join(WORK, 'icon-1024.png');
await rasterize(tileSvg, master, CANVAS);

for (const [name, width] of [
  ['32x32.png', 32],
  ['128x128.png', 128],
  ['128x128@2x.png', 256],
  ['icon.png', 512],
] as const) {
  await rasterize(tileSvg, join(ICONS, name), width);
}

await rasterize(traySvg, join(ICONS, 'tray.png'), TRAY);
await rasterize(tileSvg, join(ICONS, 'tray-linux.png'), TRAY_LINUX);
await icns(master);
await run(['magick', master, '-define', 'icon:auto-resize=256,128,64,48,32,16', join(ICONS, 'icon.ico')]);

console.log(`wrote the desktop icon set to ${ICONS}`);
