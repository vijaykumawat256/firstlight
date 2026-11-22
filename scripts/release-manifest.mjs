// Writes the update file (`latest.json`) that installed copies of Firstlight read to learn about a new version.
// Run by the Installers workflow after both installers are built:
//
//   node scripts/release-manifest.mjs <dir with the built files> <owner/repo> <tag> > latest.json
//
// The app reads `{FIRSTLIGHT_UPDATE_BASE}/{stable|beta}/latest.json`. With the base set to
// `https://github.com/<owner/repo>/releases/download`, that is the `latest.json` file on the release named `stable`
// (or `beta`), which the workflow replaces on every release. The file points at the installers on the versioned
// release, and carries each one's signature, which the app checks against the public key built into it.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

/** The version in a tag: "v1.2.3" or "v1.2.3-beta.1" (semver, as the updater compares versions). */
export function versionOf(tag) {
  const m = /^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tag);
  if (!m) throw new Error(`"${tag}" is not a version tag like v1.2.3 or v1.2.3-beta.1`);
  return m[1];
}

/** Which update file a tag goes to: a pre-release ("-beta.1", "-rc1") only reaches people on the beta channel. */
export function channelOf(tag) {
  return versionOf(tag).includes("-") ? "beta" : "stable";
}

/**
 * Builds the manifest from the list of built files and the text of their `.sig` files.
 * Mac: the universal `Firstlight.app.tar.gz` serves both kinds of Mac. Windows: the NSIS `-setup.exe` itself.
 */
export function manifest({ files, sigs, repo, tag, notes, now }) {
  const url = (name) => `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name)}`;
  const entry = (name) => {
    const sig = sigs[`${name}.sig`];
    if (!sig) throw new Error(`${name} has no .sig next to it (was TAURI_SIGNING_PRIVATE_KEY set for the build?)`);
    return { signature: sig.trim(), url: url(name) };
  };
  const mac = files.find((f) => f.endsWith(".app.tar.gz"));
  const win = files.find((f) => f.endsWith("-setup.exe"));
  if (!mac || !win) throw new Error(`need both a Mac .app.tar.gz and a Windows -setup.exe; found: ${files.join(", ")}`);
  const m = entry(mac);
  return {
    version: versionOf(tag),
    notes,
    pub_date: now.toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: {
      "darwin-aarch64": m,
      "darwin-x86_64": m,
      "windows-x86_64": entry(win),
    },
  };
}

function main([dir, repo, tag]) {
  if (!dir || !repo || !tag) throw new Error("usage: release-manifest.mjs <dir> <owner/repo> <tag>");
  const files = readdirSync(dir);
  const sigs = Object.fromEntries(files.filter((f) => f.endsWith(".sig")).map((f) => [f, readFileSync(join(dir, f), "utf8")]));
  const notes = `Firstlight ${versionOf(tag)}. See https://github.com/${repo}/releases/tag/${tag}`;
  process.stdout.write(`${JSON.stringify(manifest({ files, sigs, repo, tag, notes, now: new Date() }), null, 2)}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    process.stderr.write(`${e.message}\n`);
    process.exit(1);
  }
}
