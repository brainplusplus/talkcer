import { bubbleKey, type TimelineItem } from "../timeline";

const SPEAKER: Record<TimelineItem["speaker"], string> = {
  local_user: "Saya",
  remote: "Remote",
  ai: "AI",
  system: "System",
};

type Group = { speaker: TimelineItem["speaker"]; items: TimelineItem[] };

function groupsOf(items: TimelineItem[]): Group[] {
  const groups: Group[] = [];
  for (const item of items) {
    const last = groups[groups.length - 1];
    if (last && last.speaker === item.speaker) last.items.push(item);
    else groups.push({ speaker: item.speaker, items: [item] });
  }
  return groups;
}

function timeLabel(item: TimelineItem): string {
  const date = new Date(item.created_at || item.started_at_ms);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

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
      {groupsOf(items).map((group) => {
        const first = group.items[0];
        return (
          <li key={bubbleKey(first)} className={`turn ${group.speaker}`}>
            <div className="turn-meta">
              <span>{SPEAKER[group.speaker]}</span>
              {timeLabel(first) && <time className="turn-time">{timeLabel(first)}</time>}
            </div>
            <div className="turn-body">
              {group.items.map((item) => (
                <div key={bubbleKey(item)} className="line">
                  <p className={item.is_final ? "bubble" : "bubble partial"}>{item.text}</p>
                  {item.kind === "transcript" && item.is_final && (
                    <button
                      type="button"
                      className="explain quiet"
                      onClick={() => onExplain(item.id)}
                      disabled={explaining}
                    >
                      Explain
                    </button>
                  )}
                </div>
              ))}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
