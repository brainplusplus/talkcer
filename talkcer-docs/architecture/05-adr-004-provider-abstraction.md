# ADR-004: AI Provider Abstraction

## Status
Accepted

## Decision
STT, thinker/LLM, dan TTS memiliki trait/interface serta provider registry terpisah.

## Consequences
- Provider dapat local, self-hosted, atau cloud.
- Konfigurasi per capability.
- UI tidak bergantung pada provider tertentu.
