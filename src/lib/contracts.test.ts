import { describe, expect, it } from 'vitest';

import type { GenerationEvent, MessageContent } from './contracts';

/**
 * These tests exist mainly to make the compiler check that the generated
 * discriminated unions narrow on their tag. If `tsc` accepts the `switch`
 * bodies below, narrowing works; the runtime asserts keep it honest.
 */

function renderEvent(ev: GenerationEvent): string {
  switch (ev.type) {
    case 'TokenDelta':
      return `${ev.data.index}:${ev.data.text}`;
    case 'Done':
      return `done(${ev.data.stop_reason}, ${ev.data.tokens})`;
    case 'Error':
      return `error(${ev.data.error.kind})`;
    case 'Cancelled':
      return 'cancelled';
    default: {
      const never: never = ev;
      return never;
    }
  }
}

function contentText(c: MessageContent): string | null {
  switch (c.type) {
    case 'Text':
      return c.data.text;
    case 'Audio':
      return c.data.transcript;
    case 'Image':
      return c.data.caption;
    default: {
      const never: never = c;
      return never;
    }
  }
}

describe('contract discriminated unions', () => {
  it('narrows GenerationEvent on .type', () => {
    expect(renderEvent({ type: 'TokenDelta', data: { index: 3, text: 'hi' } })).toBe('3:hi');
    expect(renderEvent({ type: 'Done', data: { stop_reason: 'EndOfText', tokens: 5 } })).toBe(
      'done(EndOfText, 5)',
    );
    expect(renderEvent({ type: 'Error', data: { error: { kind: 'Internal' } } })).toBe(
      'error(Internal)',
    );
    expect(renderEvent({ type: 'Cancelled' })).toBe('cancelled');
  });

  it('narrows MessageContent on .type', () => {
    expect(contentText({ type: 'Text', data: { text: 'yo' } })).toBe('yo');
    expect(contentText({ type: 'Audio', data: { asset: 'sha', transcript: null } })).toBeNull();
    expect(contentText({ type: 'Image', data: { asset: 'sha', caption: 'a cat' } })).toBe('a cat');
  });
});
