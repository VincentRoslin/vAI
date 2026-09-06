import { useCallback, useEffect, useState } from 'react';

import {
  acquireFixed,
  downloadCancel,
  downloadPause,
  downloadResume,
  downloadStart,
  downloadsList,
  hfListFiles,
  hfSearch,
  modelDelete,
  modelRegisterLocal,
  modelsList,
  toAppError,
} from '../lib/ipc';
import type { DownloadInfo, HfGgufFile, HfModelSummary, RegisteredModel } from '../lib/contracts';
import { log } from '../lib/log';
import './Models.css';

function pct(d: DownloadInfo): number | null {
  if (!d.total_bytes) return null;
  return Math.round((Number(d.downloaded_bytes) / Number(d.total_bytes)) * 100);
}

export function Models(): React.JSX.Element {
  const [installed, setInstalled] = useState<RegisteredModel[]>([]);
  const [downloads, setDownloads] = useState<DownloadInfo[]>([]);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<HfModelSummary[]>([]);
  const [openRepo, setOpenRepo] = useState<string | null>(null);
  const [files, setFiles] = useState<HfGgufFile[]>([]);
  const [status, setStatus] = useState<string>('');
  const [searching, setSearching] = useState(false);
  const [localName, setLocalName] = useState('');
  const [localStatus, setLocalStatus] = useState('');
  const [registering, setRegistering] = useState(false);

  const refresh = useCallback(() => {
    modelsList()
      .then(setInstalled)
      .catch((e) => log.warn('models', `list: ${toAppError(e).kind}`));
    downloadsList()
      .then(setDownloads)
      .catch((e) => log.warn('models', `downloads: ${toAppError(e).kind}`));
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 1500);
    return () => clearInterval(t);
  }, [refresh]);

  async function runSearch(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    setSearching(true);
    setStatus('');
    try {
      setResults(await hfSearch(query.trim(), 20));
    } catch (err) {
      setStatus(
        `Search failed: ${toAppError(err).kind === 'BackendUnavailable' ? 'offline' : 'error'}`,
      );
      setResults([]);
    } finally {
      setSearching(false);
    }
  }

  async function toggleRepo(repo: string): Promise<void> {
    if (openRepo === repo) {
      setOpenRepo(null);
      return;
    }
    setOpenRepo(repo);
    setFiles([]);
    try {
      setFiles(await hfListFiles(repo));
    } catch (err) {
      setStatus(`Could not list files: ${toAppError(err).kind}`);
    }
  }

  async function registerLocal(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    const name = localName.trim();
    if (!name) return;
    setRegistering(true);
    setLocalStatus('');
    try {
      await modelRegisterLocal(name);
      setLocalName('');
      setLocalStatus(`Registered ${name}.`);
      log.info('models', `registered local gguf: ${name}`);
      refresh();
    } catch (err) {
      const k = toAppError(err).kind;
      setLocalStatus(
        k === 'NotFound'
          ? 'Not found — put the file directly in the models folder first.'
          : k === 'Validation'
            ? 'Rejected — needs a bare filename of a valid .gguf in the models folder.'
            : `Register failed: ${k}`,
      );
    } finally {
      setRegistering(false);
    }
  }

  async function startDownload(repo: string, file: HfGgufFile): Promise<void> {
    try {
      await downloadStart(
        { repo, filename: file.filename, size: file.size, sha256: file.sha256 },
        () => refresh(),
      );
      refresh();
    } catch (err) {
      setStatus(`Download refused: ${toAppError(err).kind}`);
    }
  }

  return (
    <section className="models">
      <h1>Models</h1>

      <section className="models__block">
        <h2>Installed</h2>
        {installed.length === 0 && <p className="models__empty">No models yet.</p>}
        <ul className="models__list">
          {installed.map((m) => (
            <li key={m.metadata.id} className="models__row">
              <span className="models__name">{m.metadata.display_name}</span>
              <span className="models__tag">{m.metadata.kind}</span>
              {m.metadata.quant && <span className="models__tag">{m.metadata.quant}</span>}
              <span
                className="models__tag"
                data-missing={m.availability === 'Missing' ? 'true' : undefined}
              >
                {m.availability}
              </span>
              <button
                className="models__btn"
                onClick={() => {
                  void modelDelete(m.metadata.id).then(refresh);
                }}
              >
                Delete
              </button>
            </li>
          ))}
        </ul>
        <div className="models__fixed">
          <button className="models__btn" onClick={() => void acquireFixed('Stt').then(refresh)}>
            Get speech-to-text model
          </button>
          <button className="models__btn" onClick={() => void acquireFixed('Tts').then(refresh)}>
            Get text-to-speech model
          </button>
        </div>

        <form onSubmit={registerLocal} className="models__search">
          <input
            value={localName}
            onChange={(e) => setLocalName(e.target.value)}
            placeholder="my-model.Q6_K.gguf — a file already in the models folder"
            aria-label="Local GGUF filename"
          />
          <button className="models__btn" type="submit" disabled={registering || !localName.trim()}>
            {registering ? 'Registering…' : 'Register local GGUF'}
          </button>
        </form>
        {localStatus && <p className="models__status">{localStatus}</p>}
      </section>

      {downloads.length > 0 && (
        <section className="models__block">
          <h2>Downloads</h2>
          <ul className="models__list">
            {downloads.map((d) => (
              <li key={d.id} className="models__row">
                <span className="models__name">
                  {d.repo}/{d.filename}
                </span>
                <span className="models__tag">{d.state}</span>
                {pct(d) !== null && <span className="models__tag">{pct(d)}%</span>}
                {d.error && <span className="models__err">{d.error}</span>}
                {d.state === 'Downloading' && (
                  <button
                    className="models__btn"
                    onClick={() => void downloadPause(d.id).then(refresh)}
                  >
                    Pause
                  </button>
                )}
                {d.state === 'Paused' && (
                  <button
                    className="models__btn"
                    onClick={() => void downloadResume(d.id).then(refresh)}
                  >
                    Resume
                  </button>
                )}
                {d.state !== 'Complete' && (
                  <button
                    className="models__btn"
                    onClick={() => void downloadCancel(d.id).then(refresh)}
                  >
                    Cancel
                  </button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="models__block">
        <h2>Find a model on HuggingFace</h2>
        <form onSubmit={runSearch} className="models__search">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="e.g. qwen2.5 instruct"
            aria-label="Search HuggingFace"
          />
          <button className="models__btn" type="submit" disabled={searching || !query.trim()}>
            {searching ? 'Searching…' : 'Search'}
          </button>
        </form>
        {status && <p className="models__status">{status}</p>}
        <ul className="models__list">
          {results.map((r) => (
            <li key={r.repo} className="models__result">
              <button className="models__repo" onClick={() => void toggleRepo(r.repo)}>
                {r.repo}
                {r.downloads != null && (
                  <span className="models__tag">{Number(r.downloads).toLocaleString()} ↓</span>
                )}
              </button>
              {openRepo === r.repo && (
                <ul className="models__files">
                  {files.length === 0 && <li className="models__empty">Reading files…</li>}
                  {files.map((f) => (
                    <li key={f.filename} className="models__file">
                      <span>{f.filename}</span>
                      {f.quant && <span className="models__tag">{f.quant}</span>}
                      {f.context_length && (
                        <span className="models__tag">{f.context_length.toLocaleString()} ctx</span>
                      )}
                      {f.size != null && (
                        <span className="models__tag">{(Number(f.size) / 1e9).toFixed(1)} GB</span>
                      )}
                      <button className="models__btn" onClick={() => void startDownload(r.repo, f)}>
                        Download
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </li>
          ))}
        </ul>
      </section>
    </section>
  );
}
