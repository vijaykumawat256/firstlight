// Generates src/assets/licences.json: every third-party package that ships in the app, with its licence text.
//   * Rust crates: from `cargo metadata`, restricted to the platforms we ship and to normal (non-dev, non-build)
//     dependencies of the `daybrief` crate.
//   * npm packages: production dependencies only (what ends up in the app's JavaScript).
// Run `npm run licences` before each release.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const LICENCE_FILE = /^(licen[cs]e|copying|unlicen[cs]e|notice)(\.|-|_|$)/i;

function licenceText(dir) {
  if (!dir || !existsSync(dir)) return "";
  const names = readdirSync(dir).filter((f) => LICENCE_FILE.test(f)).sort();
  return names.map((f) => readFileSync(join(dir, f), "utf8").trim()).join("\n\n---\n\n");
}

// ---- Rust -------------------------------------------------------------------------------------
const rust = new Map();
for (const platform of ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"]) {
  const meta = JSON.parse(
    execFileSync("cargo", ["metadata", "--format-version", "1", "--locked", "--filter-platform", platform], { cwd: root, maxBuffer: 1 << 28 }).toString(),
  );
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const start = meta.packages.find((p) => p.name === "daybrief").id;
  const seen = new Set();
  const stack = [start];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      // dep_kinds with kind === null is a normal dependency; "dev" and "build" are not shipped.
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  for (const id of seen) {
    const p = byId.get(id);
    if (p.name === "daybrief" || p.name === "daybrief-core") continue;
    const key = `${p.name}@${p.version}`;
    if (!rust.has(key)) {
      rust.set(key, { name: p.name, version: p.version, license: p.license ?? "see source", repository: p.repository ?? null, text: licenceText(dirname(p.manifest_path)) });
    }
  }
}

// ---- npm --------------------------------------------------------------------------------------
const npm = new Map();
const tree = JSON.parse(execFileSync("npm", ["ls", "--omit=dev", "--all", "--json"], { cwd: root, maxBuffer: 1 << 28 }).toString());
(function walk(deps) {
  for (const [name, info] of Object.entries(deps ?? {})) {
    const dir = join(root, "node_modules", name);
    const pkgPath = join(dir, "package.json");
    if (existsSync(pkgPath)) {
      const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
      const key = `${name}@${pkg.version}`;
      if (!npm.has(key)) {
        const licence = typeof pkg.license === "string" ? pkg.license : (pkg.license?.type ?? "see source");
        npm.set(key, { name, version: pkg.version, license: licence, repository: typeof pkg.repository === "string" ? pkg.repository : (pkg.repository?.url ?? null), text: licenceText(dir) });
      }
    }
    walk(info.dependencies);
  }
})(tree.dependencies);

const sort = (m) => [...m.values()].sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
// Many packages share identical text (the Apache-2.0 licence appears hundreds of times): store each text once.
const texts = [];
const index = new Map();
const compact = (p) => {
  let t = -1;
  if (p.text) {
    if (!index.has(p.text)) index.set(p.text, texts.push(p.text) - 1);
    t = index.get(p.text);
  }
  return { n: p.name, v: p.version, l: p.license, r: p.repository, t };
};
const all = { rust: sort(rust), npm: sort(npm) };
const out = { generated: new Date().toISOString().slice(0, 10), texts, rust: all.rust.map(compact), npm: all.npm.map(compact) };
mkdirSync(join(root, "src/assets"), { recursive: true });
writeFileSync(join(root, "src/assets/licences.json"), JSON.stringify(out));
const missing = [...all.rust, ...all.npm].filter((p) => !p.text);
console.log(`${out.rust.length} Rust crates, ${out.npm.length} npm packages; ${missing.length} without a licence file in the package`);
if (missing.length) console.log("  without text: " + missing.slice(0, 15).map((p) => `${p.name}@${p.version} (${p.license})`).join(", "));
