import { describe, expect, it } from "vitest";
import {
  applyServerEvent,
  bubbleKey,
  reconnectDelayMs,
  type TimelineItem,
  type TranscriptUpdated,
} from "./timeline";

function transcript(id: string, text: string, isFinal = false): TimelineItem {
  return {
    id,
    session_id: "session-1",
    kind: "transcript",
    speaker: "remote",
    source: "system_audio",
    purpose: "conversation",
    text,
    started_at_ms: 12000,
    ended_at_ms: null,
    is_final: isFinal,
    metadata: {},
    created_at: "2026-10-11T00:00:00Z",
    utterance_id: id,
  };
}

function updated(id: string, text: string, isFinal = false): TranscriptUpdated {
  return {
    type: "transcript.updated",
    session_id: "session-1",
    utterance_id: id,
    speaker: "remote",
    source: "system_audio",
    started_at_ms: 12000,
    ended_at_ms: isFinal ? 15800 : null,
    text,
    is_final: isFinal,
  };
}

describe("timeline reducer", () => {
  it("keeps three partials in one bubble and lets the final replace the text", () => {
    const id = "utterance-1";
    const first = applyServerEvent([], updated(id, "How"));
    const second = applyServerEvent(first, updated(id, "How would"));
    const third = applyServerEvent(second, updated(id, "How would you"));
    expect(third).toHaveLength(1);
    expect(bubbleKey(third[0])).toBe(id);
    expect(third[0]?.text).toBe("How would you");
    expect(third[0]?.is_final).toBe(false);

    const finalItems = applyServerEvent(third, updated(id, "How would you handle retries?", true));
    expect(finalItems).toHaveLength(1);
    expect(finalItems[0]?.text).toBe("How would you handle retries?");
    expect(finalItems[0]?.is_final).toBe(true);

    const ignored = applyServerEvent(finalItems, updated(id, "late"));
    expect(ignored[0]?.text).toBe("How would you handle retries?");
  });

  it("adds the assistant response as a separate bubble", () => {
    const transcriptItems = [transcript("utterance-1", "retries", true)];
    const withDelta = applyServerEvent(transcriptItems, {
      type: "assistant.delta",
      session_id: "session-1",
      message_id: "message-1",
      delta: "Stub response",
    });
    expect(withDelta).toHaveLength(2);
    expect(withDelta[1]?.kind).toBe("ai_message");
    expect(withDelta[1]?.text).toBe("Stub response");

    const completed = applyServerEvent(withDelta, {
      type: "assistant.completed",
      session_id: "session-1",
      message_id: "message-1",
    });
    expect(completed[1]?.is_final).toBe(true);

    const duplicate = applyServerEvent(completed, {
      type: "assistant.delta",
      session_id: "session-1",
      message_id: "message-1",
      delta: " again",
    });
    expect(duplicate[1]?.text).toBe("Stub response");
  });
});

describe("websocket reconnect", () => {
  it("backs off and caps the delay", () => {
    expect(reconnectDelayMs(0)).toBe(500);
    expect(reconnectDelayMs(1)).toBe(1000);
    expect(reconnectDelayMs(2)).toBe(2000);
    expect(reconnectDelayMs(5)).toBe(10000);
    expect(reconnectDelayMs(10)).toBe(10000);
  });
});
