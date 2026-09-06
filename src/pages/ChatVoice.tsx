import { useCallback, useEffect, useRef, useState } from 'react';

import type {
  Conversation,
  GenerationEvent,
  LifecycleStatus,
  Message,
  Persona,
  PromptPreview,
  RegisteredModel,
  VoiceState,
} from '../lib/contracts';
import {
  chatCancel,
  chatPromptPreview,
  chatSend,
  conversationCreate,
  conversationList,
  conversationMessages,
  conversationSetPersona,
  lifecycleStatus,
  modelLoad,
  modelsList,
  modelUnload,
  personaList,
  toAppError,
  voiceStart,
  voiceStop,
} from '../lib/ipc';
import { log } from '../lib/log';
import './ChatVoice.css';

type Streaming = { text: string; error: string | null };

/** Tab 1 — text chat (Phase 16 vertical slice). Voice is Phases 18–19. */
export function ChatVoice(): React.JSX.Element {
  const [models, setModels] = useState<LifecycleStatus[]>([]);
  const [registered, setRegistered] = useState<RegisteredModel[]>([]);
  const [modelBusy, setModelBusy] = useState(false);
  const [convo, setConvo] = useState<Conversation | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState<Streaming | null>(null);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [voice, setVoice] = useState<VoiceState['kind']>('Idle');
  const [personas, setPersonas] = useState<Persona[]>([]);
  const [preview, setPreview] = useState<PromptPreview | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const personaLocked = messages.length > 0 || !!streaming;

  const loaded = models.find((m) => m.state === 'Loaded' || m.state === 'Busy');
  const llms = registered.filter((r) => r.metadata.kind === 'Llm' && r.availability === 'Ready');
  const nameOf = (id: string): string =>
    registered.find((r) => r.metadata.id === id)?.metadata.display_name ?? id;

  const refreshModels = useCallback(async () => {
    try {
      const [status, list] = await Promise.all([lifecycleStatus(), modelsList()]);
      setModels(status);
      setRegistered(list);
    } catch (e) {
      log.warn('chat', `model refresh: ${toAppError(e).kind}`);
    }
  }, []);

  const loadPersonas = useCallback(async () => {
    try {
      setPersonas(await personaList());
    } catch (e) {
      log.warn('chat', `persona list: ${toAppError(e).kind}`);
    }
  }, []);

  // Bootstrap: restore the latest conversation (or create one), load models.
  useEffect(() => {
    void (async () => {
      try {
        const list = await conversationList();
        const c = list[0] ?? (await conversationCreate());
        setConvo(c);
        setMessages(await conversationMessages(c.id));
      } catch (e) {
        setNotice(`Could not open a conversation: ${toAppError(e).kind}`);
      }
      await refreshModels();
      await loadPersonas();
    })();
  }, [refreshModels, loadPersonas]);

  useEffect(() => {
    const t = setInterval(() => void refreshModels(), 2000);
    return () => clearInterval(t);
  }, [refreshModels]);

  useEffect(() => {
    const el = scrollRef.current;
    el?.scrollTo?.({ top: el.scrollHeight });
  }, [messages, streaming]);

  /** Load `id` (unloading whatever is currently loaded first). `''` = unload. */
  async function switchModel(id: string): Promise<void> {
    const current = loaded?.id ?? '';
    if (id === current || modelBusy || streaming) return;
    setModelBusy(true);
    setNotice(id ? `Loading ${nameOf(id)}…` : 'Unloading…');
    try {
      if (loaded) {
        log.info('ui', `model unload: ${loaded.id}`);
        await modelUnload(loaded.id);
      }
      if (id) {
        log.info('ui', `model load: ${id}`);
        await modelLoad(id);
      }
      setNotice(null);
    } catch (e) {
      const k = toAppError(e).kind;
      log.warn('ui', `model switch failed: ${k}`);
      setNotice(
        k === 'ResourceExhausted' ? 'Not enough VRAM for that model.' : `Model load failed: ${k}`,
      );
    } finally {
      await refreshModels();
      setModelBusy(false);
    }
  }

  async function send(): Promise<void> {
    const text = draft.trim();
    if (!text || !convo || !loaded || streaming) return;
    setDraft('');
    setMessages((m) => [...m, optimisticUser(convo.id, text)]);
    setStreaming({ text: '', error: null });
    log.info('ui', `send (${text.length} chars)`);

    try {
      const id = await chatSend(
        { conversationId: convo.id, modelId: loaded.id, text },
        (ev: GenerationEvent) => onEvent(ev),
      );
      setTaskId(id);
    } catch (e) {
      const err = toAppError(e);
      setStreaming(null);
      setNotice(
        err.kind === 'Conflict' ? 'A reply is still generating.' : `Send failed: ${err.kind}`,
      );
      await reloadMessages();
    }
  }

  function onEvent(ev: GenerationEvent): void {
    if (ev.type === 'TokenDelta') {
      setStreaming((s) => (s ? { ...s, text: s.text + ev.data.text } : s));
    } else if (ev.type === 'Done' || ev.type === 'Cancelled') {
      finishStream(null);
    } else if (ev.type === 'Error') {
      finishStream(typeof ev.data.error === 'object' ? ev.data.error.kind : 'Error');
    }
  }

  function finishStream(error: string | null): void {
    setStreaming((s) => (s ? { ...s, error } : s));
    setTaskId(null);
    // Let the Rust side finish persisting, then refetch the canonical transcript.
    setTimeout(() => void reloadMessages(), 150);
  }

  const reloadMessages = useCallback(async (): Promise<void> => {
    if (!convo) return;
    try {
      setMessages(await conversationMessages(convo.id));
      setStreaming(null);
    } catch (e) {
      log.warn('chat', `reload: ${toAppError(e).kind}`);
    }
  }, [convo]);

  async function stop(): Promise<void> {
    if (taskId) await chatCancel(taskId).catch(() => undefined);
  }

  // While a voice session runs, poll the canonical transcript so the user sees
  // turns + replies land (the Rust loop drives generation + TTS server-side).
  useEffect(() => {
    if (voice === 'Idle') return undefined;
    const t = setInterval(() => void reloadMessages(), 800);
    return () => clearInterval(t);
  }, [voice, reloadMessages]);

  async function toggleVoice(): Promise<void> {
    if (!convo) return;
    if (voice !== 'Idle') {
      log.info('ui', 'voice: stop');
      await voiceStop().catch(() => undefined);
      setVoice('Idle');
      void reloadMessages();
      return;
    }
    setNotice(null);
    log.info('ui', `voice: start (model ${loaded ? 'loaded' : 'none'})`);
    try {
      await voiceStart(convo.id, loaded?.id ?? null, (s) => setVoice(s.kind));
    } catch (e) {
      setVoice('Idle');
      log.warn('ui', `voice start failed: ${toAppError(e).kind}`);
      setNotice(`Voice unavailable: ${toAppError(e).kind}`);
    }
  }

  async function newChat(): Promise<void> {
    if (voice !== 'Idle') return;
    setNotice(null);
    setPreview(null);
    try {
      const c = await conversationCreate();
      setConvo(c);
      setMessages([]);
      setDraft('');
      await loadPersonas();
      log.info('ui', 'new conversation');
    } catch (e) {
      setNotice(`Could not start a chat: ${toAppError(e).kind}`);
    }
  }

  async function changePersona(personaId: string | null): Promise<void> {
    if (!convo || personaLocked) return;
    try {
      await conversationSetPersona(convo.id, personaId);
      setConvo({ ...convo, persona_id: personaId });
      setPreview(null);
      log.info('ui', `persona set: ${personaId ?? 'none'}`);
    } catch (e) {
      setNotice(`Could not set persona: ${toAppError(e).kind}`);
    }
  }

  async function showPrompt(open: boolean): Promise<void> {
    if (!open || !convo || !loaded) return;
    try {
      setPreview(await chatPromptPreview(convo.id, loaded.id));
    } catch (e) {
      setNotice(`Prompt preview failed: ${toAppError(e).kind}`);
    }
  }

  return (
    <section className="chat">
      <header className="chat__bar">
        <span className="chat__model">
          <span className={`chat__dot ${loaded ? 'chat__dot--on' : ''}`} />
          <select
            className="chat__modelselect"
            value={loaded?.id ?? ''}
            disabled={modelBusy || !!streaming}
            onChange={(e) => void switchModel(e.target.value)}
            title={modelBusy || streaming ? 'Busy' : 'Load / switch the language model'}
          >
            <option value="">
              {llms.length === 0 ? 'No models — add a GGUF (Models tab)' : 'No model loaded'}
            </option>
            {loaded && !llms.some((m) => m.metadata.id === loaded.id) && (
              <option value={loaded.id}>{nameOf(loaded.id)} (unavailable)</option>
            )}
            {llms.map((m) => (
              <option key={m.metadata.id} value={m.metadata.id}>
                {m.metadata.display_name}
              </option>
            ))}
          </select>
          {modelBusy ? (
            <span className="chat__meta">working…</span>
          ) : loaded?.vram_mb ? (
            <span className="chat__meta">{loaded.vram_mb} MB</span>
          ) : null}
        </span>
        <span className="chat__bar-actions">
          <button
            type="button"
            onClick={() => void newChat()}
            disabled={voice !== 'Idle' || messages.length === 0}
            title="Start a fresh conversation (needed to pick a different persona)"
          >
            New chat
          </button>
        </span>
      </header>

      {notice && <div className="chat__notice">{notice}</div>}

      <div className="chat__personabar">
        <label>
          Persona{' '}
          <select
            value={convo?.persona_id ?? ''}
            disabled={!convo || personaLocked}
            onFocus={() => void loadPersonas()}
            onChange={(e) => void changePersona(e.target.value || null)}
            title={
              personaLocked
                ? 'Fixed once the conversation has a message — start a new chat to change it'
                : undefined
            }
          >
            <option value="">Default assistant</option>
            {personas.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        {loaded && (
          <details onToggle={(e) => void showPrompt((e.target as HTMLDetailsElement).open)}>
            <summary>Show prompt</summary>
            {preview ? (
              <>
                <p className="chat__meta">
                  {preview.provenance.total_tokens}/{preview.provenance.budget_tokens} tok · persona{' '}
                  {preview.provenance.persona} · memory {preview.provenance.memory_items} (
                  {preview.provenance.memory_tokens} tok) · history{' '}
                  {preview.provenance.history_turns_included} in /{' '}
                  {preview.provenance.history_turns_dropped} dropped
                </p>
                <pre className="chat__promptpreview">{preview.prompt}</pre>
              </>
            ) : (
              <p className="chat__meta">Loading…</p>
            )}
          </details>
        )}
      </div>

      <div className="chat__transcript" ref={scrollRef}>
        {messages.length === 0 && !streaming && (
          <p className="chat__empty">Send a message to start.</p>
        )}
        {messages.map((m) => (
          <Bubble key={m.id} role={m.role} text={textOf(m)} meta={metaLine(m)} />
        ))}
        {streaming && (
          <Bubble
            role="Assistant"
            text={streaming.text || '…'}
            meta={streaming.error ? `failed: ${streaming.error}` : 'generating…'}
            streaming
          />
        )}
      </div>

      <form
        className="chat__composer"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
          placeholder={loaded ? 'Message…' : 'Load a model to chat'}
          rows={2}
          disabled={!loaded}
        />
        <button
          type="button"
          className={`chat__mic ${voice !== 'Idle' ? 'chat__mic--live' : ''}`}
          disabled={!convo || (!!streaming && voice === 'Idle')}
          onClick={() => void toggleVoice()}
          aria-label={voice === 'Idle' ? 'Start voice' : 'Stop voice'}
          title={
            voice === 'Idle'
              ? loaded
                ? 'Voice chat — speak, get a spoken reply, talk over it to interrupt'
                : 'Voice input — speak one message'
              : `Voice: ${voice} — click to stop`
          }
        >
          {voice === 'Idle' ? '🎤' : voiceLabel(voice)}
        </button>
        {streaming && !streaming.error ? (
          <button type="button" onClick={() => void stop()}>
            Stop
          </button>
        ) : (
          <button type="submit" disabled={!loaded || !draft.trim()}>
            Send
          </button>
        )}
      </form>
    </section>
  );
}

function Bubble({
  role,
  text,
  meta,
  streaming,
}: {
  role: Message['role'];
  text: string;
  meta: string;
  streaming?: boolean;
}): React.JSX.Element {
  const mine = role === 'User';
  return (
    <div className={`chat__row ${mine ? 'chat__row--user' : 'chat__row--assistant'}`}>
      <div className={`chat__bubble ${streaming ? 'chat__bubble--streaming' : ''}`}>{text}</div>
      <div className="chat__meta">{meta}</div>
    </div>
  );
}

function voiceLabel(v: VoiceState['kind']): string {
  switch (v) {
    case 'Warming':
      return '…';
    case 'Listening':
      return '👂';
    case 'Speech':
      return '🗣';
    case 'Transcribing':
      return '✍️';
    case 'Thinking':
      return '💭';
    case 'Speaking':
      return '🔊';
    case 'Interrupting':
      return '✋';
    case 'Error':
      return '⚠️';
    default:
      return '🎤';
  }
}

function textOf(m: Message): string {
  return m.content.type === 'Text' ? m.content.data.text : `[${m.content.type}]`;
}

function metaLine(m: Message): string {
  if (m.role !== 'Assistant' || !m.generation) return '';
  const g = m.generation;
  const suffix =
    g.stop_reason === 'Cancelled' ? ' · stopped' : g.stop_reason === 'Error' ? ' · failed' : '';
  return `${g.tokens} tok · ${g.duration_ms} ms${suffix}`;
}

function optimisticUser(conversationId: string, text: string): Message {
  return {
    id: `pending-${Date.now()}`,
    conversation_id: conversationId,
    role: 'User',
    content: { type: 'Text', data: { text } },
    created_at: new Date().toISOString(),
    generation: null,
  };
}
