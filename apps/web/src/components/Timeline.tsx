import { bubbleKey, type TimelineItem } from "../timeline";

const SPEAKER: Record<TimelineItem["speaker"], string> = {
  local_user: "Saya",
  remote: "Remote",
  ai: "AI",
  system: "System",
};

export function Timeline({
  items,
  onExplain,
  explaining,
}: {
  items: TimelineItem[];
  onExplain: (utteranceId: string) => void;
  explaining: boolean;
}) {
  if (items.length === 0) {
    return <p className="muted">No timeline items yet.</p>;
  }

  return (
    <ol className="timeline">
      {items.map((item) => (
        <li key={bubbleKey(item)} className={item.is_final ? "bubble" : "bubble partial"}>
          <div className="meta">
            {SPEAKER[item.speaker]}
            {!item.is_final && " · partial"}
          </div>
          <p>{item.text}</p>
          {item.kind === "transcript" && item.is_final && (
            <button type="button" onClick={() => onExplain(item.id)} disabled={explaining}>
              Explain
            </button>
          )}
        </li>
      ))}
    </ol>
  );
}
