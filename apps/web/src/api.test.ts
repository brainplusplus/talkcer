import { describe, expect, it } from "vitest";
import {
  allowedStatuses,
  exportSessionPath,
  isSessionMode,
  listSessionsPath,
  overlayViewPath,
  sessionPath,
  timelinePath,
} from "./api";

describe("session mode", () => {
  it("accepts the three product modes", () => {
    expect(isSessionMode("live_interview")).toBe(true);
    expect(isSessionMode("mock_interview")).toBe(true);
    expect(isSessionMode("learning")).toBe(true);
  });

  it("rejects an unknown mode", () => expect(isSessionMode("not_a_mode")).toBe(false));
});

describe("session status transitions", () => {
  it("follows the session state machine", () => {
    expect(allowedStatuses("ready")).toEqual(["ready", "active", "archived"]);
    expect(allowedStatuses("active")).toEqual(["active", "paused", "completed", "archived"]);
    expect(allowedStatuses("paused")).toEqual(["paused", "active", "archived"]);
    expect(allowedStatuses("completed")).toEqual(["completed", "archived"]);
    expect(allowedStatuses("archived")).toEqual(["archived"]);
  });
});

describe("api request paths", () => {
  it("builds the session and timeline paths the web actually calls", () => {
    expect(listSessionsPath("live_interview")).toBe("/api/sessions?mode=live_interview");
    expect(listSessionsPath("mock_interview")).toBe("/api/sessions?mode=mock_interview");
    expect(sessionPath("abc-123")).toBe("/api/sessions/abc-123");
    expect(timelinePath("abc-123")).toBe("/api/sessions/abc-123/timeline");
    expect(overlayViewPath()).toBe("/api/overlay/view");
    expect(exportSessionPath("abc-123", "markdown")).toBe("/api/sessions/abc-123/export?format=markdown");
    expect(exportSessionPath("abc-123", "json")).toBe("/api/sessions/abc-123/export?format=json");
  });
});
