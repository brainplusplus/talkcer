import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { getTimeline } from "./api";
import { applyServerEvent, type ServerEvent, type TimelineItem } from "./timeline";
import { openSessionSocket, type ConnectionState } from "./ws";

export function useTimeline(sessionId: string) {
  const queryClient = useQueryClient();
  const [connection, setConnection] = useState<ConnectionState>("reconnecting");
  const timeline = useQuery({
    queryKey: ["timeline", sessionId],
    queryFn: () => getTimeline(sessionId),
    enabled: sessionId.length > 0,
  });

  useEffect(() => {
    if (sessionId.length === 0) {
      return;
    }
    const apply = (event: ServerEvent) => {
      queryClient.setQueryData<{ items: TimelineItem[] }>(["timeline", sessionId], (current) => ({
        items: applyServerEvent(current?.items ?? [], event),
      }));
    };
    return openSessionSocket(sessionId, apply, setConnection, () => {
      void queryClient.invalidateQueries({ queryKey: ["timeline", sessionId] });
    });
  }, [queryClient, sessionId]);

  return {
    items: timeline.data?.items ?? [],
    isLoading: timeline.isLoading,
    isError: timeline.isError,
    connection,
  };
}
