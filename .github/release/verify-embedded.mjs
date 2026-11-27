import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { brotliDecompressSync } from "node:zlib";

const [releaseDir, dist, icon] = process.argv.slice(2);
const exe = readFileSync(join(releaseDir, "stackrinse-desktop.exe"));
const pe = exe.readUInt32LE(0x3c);
assert.equal(exe.toString("ascii", 0, 2), "MZ");
assert.equal(exe.toString("ascii", pe, pe + 4), "PE\0\0");
assert.equal(exe.readUInt16LE(pe + 4), 0x8664, "PE machine must be AMD64");
assert.equal(exe.readUInt16LE(pe + 24), 0x20b, "Expected PE32+");
assert.equal(exe.readUInt16LE(pe + 24 + 68), 2, "Expected Windows GUI subsystem");

const ico = readFileSync(icon);
assert.equal(ico.readUInt16LE(2), 1);
const iconCount = ico.readUInt16LE(4);
assert(iconCount > 1, "Expected a multi-size application icon");
for (let index = 0; index < iconCount; index++) {
  const entry = 6 + 16 * index;
  const size = ico.readUInt32LE(entry + 8);
  const offset = ico.readUInt32LE(entry + 12);
  assert(size > 0 && offset + size <= ico.length);
  assert(exe.includes(ico.subarray(offset, offset + size)), "An icon image is missing from the PE");
}

const build = join(releaseDir, "build");
const generated = readdirSync(build, { recursive: true, withFileTypes: true })
  .filter(entry => entry.isDirectory() && entry.name === "tauri-codegen-assets");
assert.equal(generated.length, 1, "Expected one fresh Tauri asset directory");
const embeddedDir = join(generated[0].parentPath, generated[0].name);
const embedded = readdirSync(embeddedDir).map(name => {
  const stored = readFileSync(join(embeddedDir, name));
  assert(stored.length && exe.includes(stored), `Asset ${name} is missing from the PE`);
  return { name, bytes: brotliDecompressSync(stored) };
});

const builtIndex = readFileSync(join(dist, "index.html"), "utf8");
const entrypoints = [...builtIndex.matchAll(/(?:src|href)="([^"]+\.(?:js|css))"/g)]
  .map(([, path]) => path);
const embeddedIndex = embedded.find(asset => asset.name.endsWith(".html"));
assert(embeddedIndex && entrypoints.length >= 2, "Expected HTML, script and stylesheet entrypoints");
for (const entry of entrypoints) {
  assert(embeddedIndex.bytes.toString("utf8").includes(entry), `Missing HTML reference ${entry}`);
}

const staticAssets = readdirSync(dist, { recursive: true, withFileTypes: true })
  .filter(entry => entry.isFile() && entry.name !== "index.html");
for (const asset of staticAssets) {
  const bytes = readFileSync(join(asset.parentPath, asset.name));
  assert(embedded.some(record => record.bytes.equals(bytes)), `Vite asset ${asset.name} was not embedded`);
}
console.log(`Verified AMD64 PE32+ GUI, ${iconCount} icon images, HTML and ${staticAssets.length} embedded Vite assets.`);
