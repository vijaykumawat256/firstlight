import { describe, expect, it } from "vitest";
import { AI_PROVIDERS, aiProvider, hostOf } from "./aiProviders";

describe("AI providers", () => {
  it("are unique, have https key pages and https-host destinations", () => {
    const ids = AI_PROVIDERS.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toEqual(["anthropic", "openai", "xai", "gemini", "mistral", "groq", "openrouter", "custom", "local"]);
    for (const p of AI_PROVIDERS) {
      expect(p.defaultModel).toMatch(/^[A-Za-z0-9._:/-]+$/);
      if (p.keyUrl) expect(p.keyUrl).toMatch(/^https:\/\//);
      if (p.host) expect(p.host).toMatch(/^[a-z0-9.-]+$/);
    }
    expect(aiProvider("local").keyLabel).toBeNull();
    expect(aiProvider("anthropic").keyLabel).toBe("Anthropic API key");
  });

  it("reads the host of an https address and nothing else", () => {
    expect(hostOf("https://api.example.com/v1")).toBe("api.example.com");
    expect(hostOf("http://api.example.com/v1")).toBeNull();
    expect(hostOf("nonsense")).toBeNull();
  });
});
