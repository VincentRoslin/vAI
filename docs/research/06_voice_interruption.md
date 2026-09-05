# 06 — Voice modality interruption flows

Scope: how "barge-in" / interruption works when voice is a *modality of the one
conversation engine* (not a separate system). Downstream of text chat — research
now, build later.

Pipeline (guide Phase 18):
```
mic → capture → STT → UserMessage → conversation engine → LLM stream → TTS → speaker
```

---

## 1. What "interruption" means — enumerate the cases

| # | User action | Expected result |
| - | ----------- | --------------- |
| A | User starts talking while **TTS is playing** the assistant reply | Stop playback immediately; keep what was already spoken as the assistant message (mark truncated); begin capturing user speech |
| B | User starts talking while the **LLM is still generating** (and TTS may have started) | Cancel LLM generation (topic 02 §4); stop TTS; persist the partial assistant message as truncated; capture user speech |
| C | User presses **stop/cancel** (button or hotword) with no new speech | Cancel generation + TTS; assistant message truncated; return to idle (not listening) |
| D | User talks while **STT is still transcribing** their previous utterance | Decide: queue as a second turn, or treat as continuation (append). Simpler v1: ignore mic until STT + turn dispatch completes, with a visible "processing" state |
| E | **Silence timeout** while listening | End capture, run STT on what was captured (or discard if below a duration/energy threshold) |
| F | TTS finishes normally, no barge-in | Return to listening (in continuous mode) or idle (push-to-talk) |

---

## 2. Barge-in detection

Two modes, pick per config:
- **Push-to-talk / explicit**: user holds a key or taps mic. Trivial, reliable, no
  false triggers. Good default.
- **Open-mic (VAD-based)**: always capturing; a **voice activity detector**
  (webrtc-vad, Silero VAD) fires on speech onset → triggers interruption.
  Risk: the assistant's own TTS audio leaking into the mic re-triggers VAD
  (acoustic echo). Mitigations:
  - Duck/pause VAD sensitivity during TTS, or
  - Acoustic echo cancellation (hard; OS-level AEC on Windows via
    `AEC` APRO / WASAPI raw vs processed capture), or
  - Require headphones for open-mic mode (document it).
  **SPIKE** if open-mic is in scope for v1 — it's a big complexity jump.

---

## 3. State machine (conversation engine, modality-aware)

```
Idle ──(start listening)──► Listening ──(speech end / VAD)──► Transcribing
Transcribing ──(UserMessage)──► Generating ──(tokens)──► Speaking
Speaking ──(TTS done)──► Listening|Idle
any of {Generating, Speaking} ──(barge-in / cancel)──► Interrupting ──► Listening
```

- The state lives in **Rust** (authoritative). The frontend renders it; it does
  not own it.
- `Interrupting` is an explicit transient state: it (1) trips the LLM
  `CancellationToken`, (2) sends `cancel` to the TTS worker, (3) stops the audio
  output stream, (4) finalizes the assistant message as `truncated`, (5) flips to
  `Listening`. Only when all four ack'd → `Listening`.

---

## 4. Latency budget (why interruption feels bad if slow)

Barge-in → silence should be **< ~150–200 ms** to feel natural. Contributors:
- audio output buffer depth (keep TTS playback buffer small, e.g. 100–200 ms
  chunks, so stopping is near-instant);
- TTS worker cancellation (kill current synth chunk; don't wait for the sentence);
- LLM cancel (HTTP disconnect — fast; slot free — verify).

Design: **stream TTS in small chunks** aligned to sentence/clause boundaries so a
stop loses at most one chunk, and the already-spoken text is a clean prefix of the
message.

---

## 5. Persistence / conversation integrity

- An interrupted assistant turn is still a **real message** in history, flagged
  `truncated: true` with the token count actually produced. The LLM context for
  the next turn uses the truncated text (that's what the user heard).
- The user's interrupting utterance becomes the next `UserMessage` once STT
  returns. Ordering: assistant(truncated) → user(new). Timestamps must reflect
  reality.
- If STT then yields empty/garbage (user cleared their throat), decide: drop the
  turn and resume, or surface "didn't catch that".

---

## 6. Worker coordination

- STT and TTS are stdio JSON-lines workers (topic 03). Both need **fast cancel**:
  - TTS: `{"type":"cancel","id":...}` mid-synthesis; worker abandons the current
    chunk. If the TTS model can't interrupt a chunk, keep chunks short.
  - STT: cancel a streaming transcription; or if batch, just discard the result.
- Audio device ownership: Rust (or a thin audio module) owns capture + playback
  streams (`cpal` crate). Workers receive/return audio as file paths or framed
  bytes over stdio, **not** direct device access.
- Full-duplex: capture and playback streams open simultaneously for open-mic;
  half-duplex (playback OR capture) is much simpler for push-to-talk v1.

---

## 7. Open decisions (ADR)
- Push-to-talk only for v1, or open-mic with VAD?
- Echo handling strategy if open-mic.
- STT: streaming (partial transcripts) vs batch-on-silence.
- Hotword/wake-word at all? (adds an always-on model.)
- `cpal` vs a higher-level audio crate; WASAPI exclusive vs shared mode.

## 8. Crates / libs to evaluate
`cpal` (audio IO), `webrtc-vad` / Silero (VAD), `rubato` (resampling),
faster-whisper / whisper.cpp (STT worker), Piper / XTTS / Kokoro (TTS worker).
