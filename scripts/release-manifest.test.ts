import { describe, expect, it } from "vitest";
// @ts-expect-error: a plain .mjs script with no type declarations
import { channelOf, manifest, versionOf } from "./release-manifest.mjs";

const files = [
  "Firstlight.app.tar.gz",
  "Firstlight.app.tar.gz.sig",
  "Firstlight_0.2.0_universal.dmg",
  "Firstlight_0.2.0_x64-setup.exe",
  "Firstlight_0.2.0_x64-setup.exe.sig",
];
const sigs = { "Firstlight.app.tar.gz.sig": "MACSIG\n", "Firstlight_0.2.0_x64-setup.exe.sig": "WINSIG" };

describe("the update file", () => {
  it("reads versions and channels from tags", () => {
    expect(versionOf("v0.2.0")).toBe("0.2.0");
    expect(versionOf("v0.2.0-beta.1")).toBe("0.2.0-beta.1");
    expect(channelOf("v0.2.0")).toBe("stable");
    expect(channelOf("v0.2.0-rc1")).toBe("beta");
    expect(() => versionOf("build-12")).toThrow();
    expect(() => versionOf("v1.2")).toThrow();
  });

  it("points every platform at the versioned release with its signature", () => {
    const m = manifest({ files, sigs, repo: "me/firstlight-releases", tag: "v0.2.0", notes: "n", now: new Date("2026-11-21T10:00:00.123Z") });
    expect(m.version).toBe("0.2.0");
    expect(m.pub_date).toBe("2026-11-21T10:00:00Z");
    expect(m.platforms["darwin-aarch64"]).toEqual({
      signature: "MACSIG",
      url: "https://github.com/me/firstlight-releases/releases/download/v0.2.0/Firstlight.app.tar.gz",
    });
    expect(m.platforms["darwin-x86_64"]).toEqual(m.platforms["darwin-aarch64"]);
    expect(m.platforms["windows-x86_64"].url).toMatch(/\/v0\.2\.0\/Firstlight_0\.2\.0_x64-setup\.exe$/);
    expect(m.platforms["windows-x86_64"].signature).toBe("WINSIG");
  });

  it("refuses a build without signatures or without one of the systems", () => {
    expect(() => manifest({ files, sigs: {}, repo: "a/b", tag: "v1.0.0", notes: "", now: new Date() })).toThrow(/no \.sig/);
    expect(() => manifest({ files: files.slice(0, 2), sigs, repo: "a/b", tag: "v1.0.0", notes: "", now: new Date() })).toThrow(/both/);
  });
});
