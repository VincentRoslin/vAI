import { useEffect, useRef, useState } from 'react';

import {
  BUILTIN_PRESETS,
  compilePrompt,
  randomizeScene,
  sanitizeScene,
  type Scene,
} from '../../lib/wizard';
import { Icon } from '../Icon';
import { WizardStepBody } from './form';
import './wizard.css';

const STEPS = [
  { id: 'start', label: 'Start' },
  { id: 'person', label: 'Person' },
  { id: 'look', label: 'Look' },
  { id: 'place', label: 'Place' },
] as const;

const STORAGE_KEY = 'vai-krea-wizard';

function seedScene(): Scene {
  return structuredClone(BUILTIN_PRESETS[0]!.scene);
}

function loadScene(): Scene {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return seedScene();
    const parsed = JSON.parse(raw) as { scene?: Scene };
    if (parsed.scene) return sanitizeScene(parsed.scene);
  } catch {
    /* ignore */
  }
  return seedScene();
}

function saveScene(scene: Scene): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ scene }));
  } catch {
    /* ignore */
  }
}

export function PromptWizard({
  open,
  onOpenChange,
  onUse,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onUse: (prompt: string) => void;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const [scene, setSceneState] = useState<Scene>(loadScene);
  const [step, setStep] = useState(0);
  const [copied, setCopied] = useState(false);

  const prompt = compilePrompt(scene);
  const last = step === STEPS.length - 1;

  function setScene(next: Scene) {
    const clean = sanitizeScene(next);
    setSceneState(clean);
    saveScene(clean);
  }

  function patchScene(fn: (s: Scene) => Scene) {
    setScene(fn(scene));
  }

  useEffect(() => {
    const el = dialogRef.current;
    if (!el) return;
    if (open && !el.open) el.showModal();
    if (!open && el.open) el.close();
  }, [open]);

  useEffect(() => {
    if (open) setStep(0);
  }, [open]);

  useEffect(() => {
    bodyRef.current?.scrollTo?.({ top: 0 });
  }, [step]);

  async function copyPrompt() {
    try {
      await navigator.clipboard.writeText(prompt);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      /* ignore */
    }
  }

  function useAndClose() {
    const text = compilePrompt(scene);
    onUse(text);
    onOpenChange(false);
  }

  function loadPreset(id: string) {
    const builtin = BUILTIN_PRESETS.find((p) => p.id === id);
    if (builtin) {
      setScene(structuredClone(builtin.scene));
      setStep(1);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className="wiz"
      onClose={() => onOpenChange(false)}
      aria-labelledby="wiz-title"
    >
      <header className="wiz-head">
        <div>
          <h2 id="wiz-title" className="wiz-title">
            <Icon name="wand" size={16} />
            Prompt wizard
          </h2>
          <p className="wiz-sub">
            {step + 1} of {STEPS.length} · {STEPS[step]?.label}
          </p>
        </div>
        <button
          type="button"
          className="wiz-icon-btn"
          aria-label="Close"
          onClick={() => onOpenChange(false)}
        >
          <Icon name="x" size={16} />
        </button>
      </header>

      <div className="wiz-progress" role="tablist" aria-label="Wizard steps">
        {STEPS.map((s, i) => (
          <button
            key={s.id}
            type="button"
            role="tab"
            aria-selected={i === step}
            aria-label={`${s.label}, step ${i + 1}`}
            onClick={() => setStep(i)}
            className={i <= step ? 'is-on' : ''}
          />
        ))}
      </div>

      <div ref={bodyRef} className="wiz-body">
        <WizardStepBody step={step} scene={scene} onScene={patchScene} onPickPreset={loadPreset} />
      </div>

      <footer className="wiz-foot">
        <div className="wiz-preview">
          <p>{prompt || 'The prompt will appear here as you choose.'}</p>
          <button
            type="button"
            className="wiz-icon-btn"
            onClick={copyPrompt}
            aria-label="Copy prompt"
          >
            <Icon name={copied ? 'check' : 'copy'} size={14} />
          </button>
        </div>
        <div className="wiz-actions">
          {step === 0 ? (
            <button
              type="button"
              className="wiz-btn"
              onClick={() => {
                setScene(randomizeScene(scene));
                setStep(1);
              }}
            >
              <Icon name="dice" size={14} />
              Surprise me
            </button>
          ) : (
            <>
              <button type="button" className="wiz-btn" onClick={() => setStep((n) => n - 1)}>
                Back
              </button>
              <button
                type="button"
                className="wiz-btn wiz-btn-icon"
                onClick={() => setScene(randomizeScene(scene))}
                aria-label="Randomize"
              >
                <Icon name="dice" size={14} />
              </button>
            </>
          )}
          <span className="wiz-spacer" />
          {!last && (
            <button type="button" className="wiz-btn wiz-btn-ghost" onClick={useAndClose}>
              Use prompt
            </button>
          )}
          {last ? (
            <button type="button" className="wiz-btn wiz-btn-primary" onClick={useAndClose}>
              Use prompt
            </button>
          ) : (
            <button
              type="button"
              className="wiz-btn wiz-btn-primary"
              onClick={() => setStep((n) => n + 1)}
            >
              {step === 0 ? 'Continue' : 'Next'}
            </button>
          )}
        </div>
      </footer>
    </dialog>
  );
}
