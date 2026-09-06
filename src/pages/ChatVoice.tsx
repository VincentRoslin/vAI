import { useCallback, useEffect, useRef, useState } from 'react';

import type {
  Conversation,
  GenerationEvent,
  LifecycleStatus,
  Message,
  VoiceState,
} from '../lib/contracts';
import {
  chatCancel,
  chatGenerate,
  chatSend,
  conversationCreate,
  conversationList,
  conversationMessages,
  lifecycleStatus,
  modelLoad,
  modelRegisterLocal,
  modelsList,
  modelUnload,
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
  const [modelName, setModelName] = useState<Record<string, string>>({});
  const [convo, setConvo] = useState<Conversation | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState<Streaming | null>(null);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [voice, setVoice] = useState<VoiceState['kind']>('Idle');
  const scrollRef = useRef<HTMLDivElement>(null);

  const loaded = models.find((m) => m.state === 'Loaded' || m.state === 'Busy');

  const refreshModels = useCallback(async () => {
    try {
      const [status, registered] = await Promise.all([lifecycleStatus(), modelsList()]);
      setModels(status);
      setModelName(
        Object.fromEntries(registered.map((r) => [r.metadata.id, r.metadata.display_name])),
      );
    } catch (e) {
      log.warn('chat', `model refresh: ${toAppError(e).kind}`);
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
    })();
  }, [refreshModels]);

  useEffect(() => {
    const t = setInterval(() => void refreshModels(), 2000);
    return () => clearInterval(t);
  }, [refreshModels]);

  useEffect(() => {
    const el = scrollRef.current;
    el?.scrollTo?.({ top: el.scrollHeight });
  }, [messages, streaming]);

  async function ensureModel(): Promise<void> {
    setNotice(null);
    try {
      const registered = await modelsList();
      let gguf = registered.find((r) => r.metadata.kind === 'Llm');
      if (!gguf) {
        const id = await modelRegisterLocal('qwen2.5-0.5b-instruct-q4_k_m.gguf');
        await refreshModels();
        gguf = (await modelsList()).find((r) => r.metadata.id === id);
      }
      if (gguf) {
        setNotice('Loading model…');
        log.info('ui', `model load requested: ${gguf.metadata.id}`);
        await modelLoad(gguf.metadata.id);
        setNotice(null);
      }
    } catch (e) {
      log.warn('ui', `model load failed: ${toAppError(e).kind}`);
      setNotice(`Model load failed: ${toAppError(e).kind}`);
    }
    await refreshModels();
  }

  async function unload(): Promise<void> {
    if (loaded) {
      log.info('ui', `model unload requested: ${loaded.id}`);
      await modelUnload(loaded.id).catch((e) => setNotice(`Unload failed: ${toAppError(e).kind}`));
      await refreshModels();
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

  async function reloadMessages(): Promise<void> {
    if (!convo) return;
    try {
      setMessages(await conversationMessages(convo.id));
      setStreaming(null);
    } catch (e) {
      log.warn('chat', `reload: ${toAppError(e).kind}`);
    }
  }

  async function stop(): Promise<void> {
    if (taskId) await chatCancel(taskId).catch(() => undefined);
  }

  async function talkStart(): Promise<void> {
    if (!convo || streaming || voice !== 'Idle') return;
    setNotice(null);
    log.info('ui', 'voice: push-to-talk start');
    try {
      await voiceStart(convo.id, (s) => setVoice(s.kind));
    } catch (e) {
      setVoice('Idle');
      log.warn('ui', `voice start failed: ${toAppError(e).kind}`);
      setNotice(`Voice unavailable: ${toAppError(e).kind}`);
    }
  }

  async function talkStop(): Promise<void> {
    log.info('ui', 'voice: push-to-talk release');
    try {
      await voiceStop();
    } catch {
      /* already stopped */
    }
    setVoice('Idle');
    // The transcript is now a user turn; show it, then reply if a model is loaded.
    setTimeout(() => {
      void (async () => {
        await reloadMessages();
        if (!convo || !loaded || streaming) return;
        const msgs = await conversationMessages(convo.id).catch(() => []);
        const last = msgs.at(-1);
        if (last?.role === 'User') {
          setStreaming({ text: '', error: null });
          try {
            const id = await chatGenerate({ conversationId: convo.id, modelId: loaded.id }, (ev) =>
              onEvent(ev),
            );
            setTaskId(id);
          } catch (e) {
            setStreaming(null);
            setNotice(`Reply failed: ${toAppError(e).kind}`);
          }
        }
      })();
    }, 250);
  }

  return (
    <section className="chat">
      <header className="chat__bar">
        <span className="chat__model">
          {loaded ? (
            <>
              <span className="chat__dot chat__dot--on" /> {modelName[loaded.id] ?? loaded.id}
              {loaded.vram_mb ? ` · ${loaded.vram_mb} MB` : ''}
            </>
          ) : (
            <>
              <span className="chat__dot" /> No model loaded
            </>
          )}
        </span>
        {loaded ? (
          <button type="button" onClick={() => void unload()}>
            Unload
          </button>
        ) : (
          <button type="button" onClick={() => void ensureModel()}>
            Load model
          </button>
        )}
      </header>

      {notice && <div className="chat__notice">{notice}</div>}

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
          disabled={!convo || !!streaming}
          onPointerDown={() => void talkStart()}
          onPointerUp={() => void talkStop()}
          onPointerLeave={() => voice !== 'Idle' && void talkStop()}
          aria-label="Hold to talk"
          title="Hold to talk"
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
