// Fetches the castLabs Electron that ADR 0006 pins, checks it against the sha256 in
// electron.pin.json, and unpacks it. The Dev host and the image build use the same pin.
//
//   node engine/fetch-electron.mjs [platform-arch] [into-directory]
//
// Prints the path of the Electron executable. Run again and it does nothing.

import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pin = JSON.parse(readFileSync(join(here, "electron.pin.json"), "utf8"));
const target = process.argv[2] ?? `${process.platform}-${process.arch}`;
const asset = pin.assets[target];
if (!asset) die(`no Electron is pinned for ${target} (have: ${Object.keys(pin.assets).join(", ")})`);

const into = resolve(process.argv[3] ?? join(here, "..", ".m4tv-dev", "electron", `${pin.tag}-${target}`));
const executable = join(into, target.startsWith("win32") ? "electron.exe" : "electron");
const stamp = join(into, ".sha256");

if (existsSync(executable) && existsSync(stamp) && readFileSync(stamp, "utf8") === asset.sha256) {
  console.log(executable);
  process.exit(0);
}

const zip = `${into}.zip`;
if (!(existsSync(zip) && (await sha256(zip)) === asset.sha256)) {
  const url = `https://github.com/${pin.repository}/releases/download/${encodeURIComponent(pin.tag)}/${encodeURIComponent(asset.name)}`;
  console.error(`downloading ${url}`);
  const response = await fetch(url);
  if (!response.ok) die(`${url}: ${response.status}`);
  mkdirSync(dirname(zip), { recursive: true });
  await pipeline(Readable.fromWeb(response.body), createWriteStream(`${zip}.part`));
  const got = await sha256(`${zip}.part`);
  if (got !== asset.sha256) {
    rmSync(`${zip}.part`);
    die(`${asset.name} has sha256 ${got}, not the pinned ${asset.sha256}`);
  }
  renameSync(`${zip}.part`, zip);
}

rmSync(into, { recursive: true, force: true });
mkdirSync(into, { recursive: true });
// Windows 10+ ships bsdtar, which reads zip (by its full path: Git's GNU tar would come first on
// PATH and takes "C:" for a host name); the image build has unzip.
const [program, args] =
  process.platform === "win32"
    ? [join(process.env.SystemRoot ?? "C:\Windows", "System32", "tar.exe"), ["-xf", zip, "-C", into]]
    : ["unzip", ["-q", zip, "-d", into]];
const unpacked = spawnSync(program, args, { stdio: "inherit" });
if (unpacked.status !== 0) die(`could not unpack ${zip} with ${program}`);
if (!existsSync(executable)) die(`${executable} is not in ${zip}`);
writeFileSync(stamp, asset.sha256);
console.log(executable);

async function sha256(file) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}

function die(message) {
  console.error(`fetch-electron: ${message}`);
  process.exit(1);
}
