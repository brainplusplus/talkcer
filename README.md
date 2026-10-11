# Talkcer

AI Conversation Studio. This repository currently contains Task 1 and Task 2 only.

## Run

```powershell
cargo run --bin talkcer-cli serve
```

The API listens on `http://127.0.0.1:8787`.

```powershell
cd apps/web
npm install
npm run dev
```

Open `http://127.0.0.1:5173`. The dev server proxies `/api` to the Rust server.

## Verify

```powershell
cargo test --workspace
```

```powershell
cd apps/web
npm test
npm run build
```

Health check:

```powershell
curl.exe http://127.0.0.1:8787/api/health
```

## Timeline

Partial and final transcripts:

```http
POST /api/sessions/{id}/transcripts
GET /api/sessions/{id}/timeline
WS /ws?session_id={id}
```

A repeated `utterance_id` updates the same row. A final row rejects later updates with `409`. `assistant.action` stores a local stub and does not call a provider.
