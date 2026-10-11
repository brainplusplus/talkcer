import { reconnectDelayMs, type ServerEvent } from "./timeline";

export type ConnectionState = "open" | "reconnecting";

export function openSessionSocket(
  sessionId: string,
  onEvent: (event: ServerEvent) => void,
  onState: (state: ConnectionState) => void,
  onReconnect: () => void,
): () => void {
  let disposed = false;
  let attempt = 0;
  let timer = 0;
  let socket: WebSocket | undefined;
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const url = `${protocol}//${window.location.host}/ws?session_id=${sessionId}`;

  const connect = () => {
    socket = new WebSocket(url);
    socket.onopen = () => {
      const reconnected = attempt > 0;
      attempt = 0;
      onState("open");
      if (reconnected) {
        onReconnect();
      }
    };
    socket.onmessage = (message: MessageEvent<string>) => {
      try {
        onEvent(JSON.parse(message.data) as ServerEvent);
      } catch {
        // Ignore a frame that is not a Talkcer event.
      }
    };
    socket.onclose = () => {
      if (disposed) {
        return;
      }
      onState("reconnecting");
      timer = window.setTimeout(connect, reconnectDelayMs(attempt));
      attempt += 1;
    };
  };

  connect();
  return () => {
    disposed = true;
    window.clearTimeout(timer);
    socket?.close();
  };
}
