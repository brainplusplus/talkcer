export type Speaker = "local_user" | "remote" | "ai" | "system";
export type TimelineSource = "microphone" | "system_audio" | "text" | "ai_generated";

export type TimelineItem = {
  id: string;
  session_id: string;
  kind: "transcript" | "user_message" | "ai_message" | "feedback" | "system_event";
  speaker: Speaker;
  source: TimelineSource;
  purpose: "conversation" | "assistant_instruction" | "assistant_response";
  text: string;
  started_at_ms: number;
  ended_at_ms: number | null;
  is_final: boolean;
  metadata: Record<string, unknown>;
  created_at: string;
  utterance_id?: string;
};

export type TranscriptUpdated = {
  type: "transcript.updated";
  session_id: string;
  utterance_id: string;
  speaker: Speaker;
  source: TimelineSource;
  started_at_ms: number;
  ended_at_ms: number | null;
  text: string;
  is_final: boolean;
};

export type AssistantDelta = {
  type: "assistant.delta";
  session_id: string;
  message_id: string;
  delta: string;
};

export type AssistantCompleted = {
  type: "assistant.completed";
  session_id: string;
  message_id: string;
};

export type ServerEvent = TranscriptUpdated | AssistantDelta | AssistantCompleted | { type: string };

export function bubbleKey(item: TimelineItem): string {
  return item.utterance_id ?? item.id;
}

export function reconnectDelayMs(attempt: number): number {
  return Math.min(10_000, 500 * 2 ** Math.max(0, attempt));
}

export function applyServerEvent(items: TimelineItem[], event: ServerEvent): TimelineItem[] {
  if (event.type === "transcript.updated") {
    return applyTranscript(items, event as TranscriptUpdated);
  }
  if (event.type === "assistant.delta") {
    return applyDelta(items, event as AssistantDelta);
  }
  if (event.type === "assistant.completed") {
    return applyCompleted(items, event as AssistantCompleted);
  }
  return items;
}

function applyTranscript(items: TimelineItem[], event: TranscriptUpdated): TimelineItem[] {
  const index = items.findIndex(
    (item) => bubbleKey(item) === event.utterance_id || item.id === event.utterance_id,
  );
  if (index === -1) {
    return sortItems([
      ...items,
      {
        id: event.utterance_id,
        session_id: event.session_id,
        kind: "transcript",
        speaker: event.speaker,
        source: event.source,
        purpose: "conversation",
        text: event.text,
        started_at_ms: event.started_at_ms,
        ended_at_ms: event.ended_at_ms,
        is_final: event.is_final,
        metadata: {},
        created_at: "",
        utterance_id: event.utterance_id,
      },
    ]);
  }
  const existing = items[index];
  if (existing?.is_final && !event.is_final) {
    return items;
  }
  return items.map((item, itemIndex) =>
    itemIndex === index
      ? {
          ...item,
          text: event.text,
          is_final: event.is_final,
          ended_at_ms: event.ended_at_ms,
          utterance_id: event.utterance_id,
        }
      : item,
  );
}

function applyDelta(items: TimelineItem[], event: AssistantDelta): TimelineItem[] {
  const index = items.findIndex((item) => item.id === event.message_id);
  if (index === -1) {
    return sortItems([
      ...items,
      {
        id: event.message_id,
        session_id: event.session_id,
        kind: "ai_message",
        speaker: "ai",
        source: "ai_generated",
        purpose: "assistant_response",
        text: event.delta,
        started_at_ms: Date.now(),
        ended_at_ms: null,
        is_final: false,
        metadata: {},
        created_at: "",
      },
    ]);
  }
  const existing = items[index];
  if (existing?.is_final) {
    return items;
  }
  return items.map((item, itemIndex) =>
    itemIndex === index ? { ...item, text: `${item.text}${event.delta}` } : item,
  );
}

function applyCompleted(items: TimelineItem[], event: AssistantCompleted): TimelineItem[] {
  return items.map((item) => (item.id === event.message_id ? { ...item, is_final: true } : item));
}

function sortItems(items: TimelineItem[]): TimelineItem[] {
  return [...items].sort(
    (left, right) =>
      left.started_at_ms - right.started_at_ms || left.created_at.localeCompare(right.created_at),
  );
}
