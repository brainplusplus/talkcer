import { describe, expect, it } from "vitest";
import { allowedStatuses, isSessionMode } from "./api";

describe("session mode", () => {
  it("accepts the three product modes", () => {
    expect(isSessionMode("live_interview")).toBe(true);
    expect(isSessionMode("mock_interview")).toBe(true);
    expect(isSessionMode("learning")).toBe(true);
  });

  it("rejects an unknown mode", () => {
    expect(isSessionMode("not_a_mode")).toBe(false);
  });
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
