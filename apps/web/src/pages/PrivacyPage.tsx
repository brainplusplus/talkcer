import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getOverlay, setUndetectable } from "../api";

export function PrivacyPage() {
  const queryClient = useQueryClient();
  const overlay = useQuery({ queryKey: ["overlay"], queryFn: getOverlay });
  const undetectable = overlay.data?.undetectable ?? false;

  const toggle = useMutation({
    mutationFn: (enabled: boolean) => setUndetectable(enabled),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["overlay"] });
    },
  });

  return (
    <section className="stack">
      <h2>Privacy</h2>
      <p className="meta">
        Undetectable mode keeps Talkcer out of screen captures and recordings, hides the
        taskbar button, removes the tray icon, and replaces the window title with a neutral
        system name. The desktop shell applies it within half a second and re-asserts the
        capture exclusion, because Windows can drop it silently. Keyboard shortcuts keep
        working while the tray is hidden.
      </p>
      <button
        type="button"
        className={undetectable ? "primary" : ""}
        disabled={overlay.isLoading || toggle.isPending}
        onClick={() => toggle.mutate(!undetectable)}
      >
        {undetectable ? "Undetectable: on" : "Undetectable: off"}
      </button>
      <p className="meta">Shortcut: Ctrl+2.</p>
      {toggle.isError && <p className="error">{(toggle.error as Error).message}</p>}
    </section>
  );
}
