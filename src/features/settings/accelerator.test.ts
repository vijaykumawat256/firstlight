import { acceleratorFromEvent, prettyAccelerator } from "./accelerator";

const ev = (over: Partial<Parameters<typeof acceleratorFromEvent>[0]>) => ({ key: "", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...over });

describe("acceleratorFromEvent", () => {
  it("builds shortcuts from modifiers plus a key", () => {
    expect(acceleratorFromEvent(ev({ key: "k", code: "KeyK", metaKey: true, shiftKey: true }))).toBe("Cmd+Shift+K");
    expect(acceleratorFromEvent(ev({ key: " ", code: "Space", ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+Space");
    expect(acceleratorFromEvent(ev({ key: "F5", code: "F5", metaKey: true }))).toBe("Cmd+F5");
    expect(acceleratorFromEvent(ev({ key: "ArrowRight", metaKey: true, shiftKey: true }))).toBe("Cmd+Shift+Right");
  });
  it("uses the physical key so Option does not change the letter", () => {
    expect(acceleratorFromEvent(ev({ key: "˚", code: "KeyK", altKey: true, metaKey: true }))).toBe("Cmd+Alt+K");
  });
  it("refuses shortcuts without a real modifier", () => {
    expect(acceleratorFromEvent(ev({ key: "k", code: "KeyK" }))).toBeNull();
    expect(acceleratorFromEvent(ev({ key: "K", code: "KeyK", shiftKey: true }))).toBeNull();
  });
  it("ignores pure modifier presses and unsupported keys", () => {
    expect(acceleratorFromEvent(ev({ key: "Meta", metaKey: true }))).toBeNull();
    expect(acceleratorFromEvent(ev({ key: ";", code: "Semicolon", metaKey: true }))).toBeNull();
  });
});

describe("prettyAccelerator", () => {
  it("uses Apple's symbols", () => {
    expect(prettyAccelerator("Cmd+Shift+Space")).toBe("⌘⇧ Space");
    expect(prettyAccelerator("CmdOrCtrl+Alt+K")).toBe("⌘⌥ K");
    expect(prettyAccelerator("Ctrl+9")).toBe("⌃ 9");
  });
});
