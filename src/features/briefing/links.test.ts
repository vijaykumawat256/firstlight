import { describe, expect, it } from "vitest";
import { eventLink, firstLink, placeLabel, videoServiceName } from "./links";

describe("videoServiceName", () => {
  it("names the common services, including company sub-domains", () => {
    expect(videoServiceName("https://us02web.zoom.us/j/123")).toBe("Zoom");
    expect(videoServiceName("https://meet.google.com/abc-defg-hij")).toBe("Google Meet");
    expect(videoServiceName("https://teams.microsoft.com/l/meetup-join/x")).toBe("Microsoft Teams");
    expect(videoServiceName("https://acme.webex.com/meet/jo")).toBe("Webex");
  });
  it("falls back to the site name, and refuses anything that is not an https link", () => {
    expect(videoServiceName("https://www.example.org/room/4")).toBe("example.org");
    expect(videoServiceName("http://zoom.us/j/1")).toBeNull();
    expect(videoServiceName("javascript:alert(1)")).toBeNull();
    expect(videoServiceName("not a link")).toBeNull();
    // A look-alike is not Zoom.
    expect(videoServiceName("https://zoom.us.evil.com/j/1")).toBe("zoom.us.evil.com");
  });
});

describe("placeLabel", () => {
  it("turns a link used as the place into a name, and leaves real places alone", () => {
    expect(placeLabel("https://zoom.us/j/123?pwd=abc")).toBe("Zoom link");
    expect(placeLabel("Room 4B")).toBe("Room 4B");
    expect(placeLabel("Visit https://example.com for details")).toBe("Visit https://example.com for details");
    expect(placeLabel(null)).toBeNull();
    expect(placeLabel("")).toBeNull();
  });
});

describe("firstLink", () => {
  it("finds the first address and drops sentence punctuation after it", () => {
    expect(firstLink("Docs: https://example.com/spec.")).toBe("https://example.com/spec");
    expect(firstLink("see (https://example.com/a?b=1), then http://other.test")).toBe("https://example.com/a?b=1");
    expect(firstLink("no link here")).toBeNull();
    expect(firstLink(null)).toBeNull();
    expect(firstLink("")).toBeNull();
  });
});

describe("eventLink", () => {
  const base = { join_url: null, location: null, description: null };
  it("prefers the meeting link, then the place, then the notes", () => {
    expect(eventLink({ ...base, join_url: "https://zoom.us/j/1", location: "https://a.test", description: "https://b.test" })).toBe("https://zoom.us/j/1");
    expect(eventLink({ ...base, location: "https://a.test/room", description: "https://b.test" })).toBe("https://a.test/room");
    expect(eventLink({ ...base, description: "Agenda: https://b.test/agenda" })).toBe("https://b.test/agenda");
    expect(eventLink(base)).toBeNull();
  });
});
