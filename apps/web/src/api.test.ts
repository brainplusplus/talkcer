import { describe, expect, it } from "vitest";
import {
  allowedStatuses,
  apiBase,
  audioDevicesPath,
  captureDevicesPath,
  capturePath,
  exportSessionPath,
  isDesktopShell,
  isSessionMode,
  listSessionsPath,
  overlayViewPath,
  resolveApiBase,
  sessionPath,
  timelinePath,
  wsBase,
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
    expect(capturePath("abc-123")).toBe("/api/sessions/abc-123/capture");
    expect(captureDevicesPath("abc-123")).toBe("/api/sessions/abc-123/capture/devices");
    expect(audioDevicesPath()).toBe("/api/devices/audio");
    expect(exportSessionPath("abc-123", "markdown")).toBe("/api/sessions/abc-123/export?format=markdown");
    expect(exportSessionPath("abc-123", "json")).toBe("/api/sessions/abc-123/export?format=json");
  });
});

describe("desktop bridge", () => {
  it("reports no desktop shell without a browser", () => {
    expect(isDesktopShell()).toBe(false);
  });

  it("keeps a relative base on a normal origin", () => {
    expect(resolveApiBase({ host: "localhost:5173", protocol: "http:" })).toBe("");
  });

  it("maps a tauri origin to the local core API", () => {
    expect(resolveApiBase({ host: "tauri.localhost", protocol: "http:" })).toBe(
      "http://127.0.0.1:54000",
    );
    expect(wsBase()).toBe(apiBase().replace(/^http/, "ws"));
  });

  it("honours an explicitly configured API base", () => {
    expect(
      resolveApiBase({ host: "anything", protocol: "http:", configured: "http://10.0.0.2:54001/" }),
    ).toBe("http://10.0.0.2:54001");
  });
});
