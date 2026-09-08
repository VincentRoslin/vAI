import type { ReactNode } from 'react';

import { visibleOpts, type Opt } from '../../lib/wizard';
import { Icon } from '../Icon';

export function cx(...xs: Array<string | false | undefined | null>): string {
  return xs.filter(Boolean).join(' ');
}

export function FieldLabel({ children }: { children: ReactNode }) {
  return <span className="wiz-label">{children}</span>;
}

export function ChoiceGrid({
  label,
  options,
  value,
  onChange,
  nsfw,
}: {
  label: string;
  options: Opt[];
  value?: string;
  onChange: (id: string | undefined) => void;
  nsfw: boolean;
}) {
  const list = visibleOpts(options, nsfw);
  if (!list.length) return null;
  return (
    <div className="wiz-field">
      <FieldLabel>{label}</FieldLabel>
      <div className="wiz-choice-grid">
        {list.map((o) => {
          const on = value === o.id;
          return (
            <button
              key={o.id}
              type="button"
              onClick={() => onChange(on ? undefined : o.id)}
              className={cx('wiz-choice', on && 'is-on')}
            >
              {o.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function ChipSelect({
  label,
  options,
  value,
  onChange,
  nsfw,
}: {
  label: string;
  options: Opt[];
  value?: string;
  onChange: (id: string | undefined) => void;
  nsfw: boolean;
}) {
  const list = visibleOpts(options, nsfw);
  if (!list.length) return null;
  if (list.length > 5) {
    return (
      <SelectField label={label} options={options} value={value} onChange={onChange} nsfw={nsfw} />
    );
  }
  return (
    <div className="wiz-field">
      <FieldLabel>{label}</FieldLabel>
      <div className="wiz-chips">
        {list.map((o) => {
          const on = value === o.id;
          return (
            <button
              key={o.id}
              type="button"
              onClick={() => onChange(on ? undefined : o.id)}
              className={cx('wiz-chip', on && 'is-on')}
            >
              {o.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function ChipMulti({
  label,
  options,
  values,
  onChange,
  nsfw,
  max = 3,
}: {
  label: string;
  options: Opt[];
  values: string[];
  onChange: (ids: string[]) => void;
  nsfw: boolean;
  max?: number;
}) {
  const list = visibleOpts(options, nsfw);
  return (
    <div className="wiz-field">
      <FieldLabel>{label}</FieldLabel>
      <div className="wiz-chips">
        {list.map((o) => {
          const on = values.includes(o.id);
          return (
            <button
              key={o.id}
              type="button"
              onClick={() => {
                if (on) onChange(values.filter((id) => id !== o.id));
                else if (values.length < max) onChange([...values, o.id]);
              }}
              className={cx('wiz-chip', on && 'is-on')}
            >
              {o.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function SelectField({
  label,
  options,
  value,
  onChange,
  nsfw,
  allowEmpty = true,
}: {
  label: string;
  options: Opt[];
  value?: string;
  onChange: (id: string | undefined) => void;
  nsfw: boolean;
  allowEmpty?: boolean;
}) {
  const list = visibleOpts(options, nsfw);
  if (!list.length) return null;
  return (
    <label className="wiz-field">
      <FieldLabel>{label}</FieldLabel>
      <select
        value={value ?? ''}
        onChange={(e) => onChange(e.target.value || undefined)}
        className="wiz-select"
      >
        {allowEmpty && <option value="">Any</option>}
        {list.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

export function CustomRow({
  value,
  onChange,
  placeholder,
}: {
  value?: string;
  onChange: (v: string) => void;
  placeholder: string;
}) {
  return (
    <input
      value={value ?? ''}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="wiz-select"
    />
  );
}

export function More({
  children,
  label = 'More options',
}: {
  children: ReactNode;
  label?: string;
}) {
  return (
    <details className="wiz-more">
      <summary>
        <Icon name="chevron-down" size={14} />
        {label}
      </summary>
      <div className="wiz-more-body">{children}</div>
    </details>
  );
}
