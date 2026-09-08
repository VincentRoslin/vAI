import { describe, expect, it } from 'vitest';

import { compilePrompt, looksLikeKeywordSalad } from './compiler.ts';
import { BUILTIN_PRESETS } from './presets.ts';
import { randomizeScene } from './randomize.ts';
import { emptyGarment, emptyScene, sanitizeScene } from './scene.ts';
import type { Scene } from './types.ts';

function specScene(): Scene {
  const s = emptyScene();
  const ch = s.characters[0]!;
  ch.age = '25';
  ch.gender = 'woman';
  ch.hairColor = 'dark_brown';
  ch.hairLength = 'long';
  ch.hairTexture = 'wavy';
  ch.eyeColor = 'blue';
  ch.bodyType = 'slim';
  ch.garments = [
    { ...emptyGarment('top', 'tshirt'), color: 'white', fit: 'fitted', material: 'cotton' },
    { ...emptyGarment('bottom', 'jeans'), color: 'dark_blue' },
    { ...emptyGarment('accessory', 'jewelry'), color: 'silver' },
  ];
  ch.position = 'standing';
  ch.arms = 'pocket';
  ch.gaze = 'camera';
  ch.expression = 'slight_smile';
  s.location = 'bedroom';
  s.time = 'sunset';
  s.lightSource = 'window';
  s.lightQuality = 'soft';
  s.lightTemp = 'warm';
  s.framing = 'head';
  s.lens = '85';
  s.dof = 'shallow';
  s.medium = 'photo';
  s.photoStyle = 'editorial';
  s.finish = 'cinematic_f';
  s.colorMood = 'muted_warm';
  s.moods = ['intimate'];
  return s;
}

describe('Krea 2 compiler', () => {
  it('compiles the spec example to fluent prose, not keyword salad', () => {
    const text = compilePrompt(specScene());
    expect(text.length).toBeGreaterThan(80);
    expect(text).toMatch(/25-year-old woman/i);
    expect(text).toMatch(/long wavy dark brown hair/i);
    expect(text).toMatch(/blue eyes/i);
    expect(text).toMatch(/T-shirt/i);
    expect(text).toMatch(/jeans/i);
    expect(text).toMatch(/85mm/i);
    expect(looksLikeKeywordSalad(text)).toBe(false);
  });

  it('skips street clothes for nude attire', () => {
    const s = specScene();
    s.nsfw = true;
    s.characters[0]!.nsfwAttire = 'nude';
    const text = compilePrompt(s);
    expect(text).toMatch(/nude/i);
    expect(text).not.toMatch(/T-shirt/i);
    expect(text).not.toMatch(/jeans/i);
    expect(text).toMatch(/jewelry|silver/i);
  });

  it('omits top and bottom when a dress is present', () => {
    const s = specScene();
    s.characters[0]!.garments.push({
      ...emptyGarment('dress', 'slip'),
      color: 'black',
      material: 'silk',
    });
    const text = compilePrompt(s);
    expect(text).toMatch(/slip dress/i);
    expect(text).not.toMatch(/T-shirt/i);
    expect(text).not.toMatch(/jeans/i);
  });

  it('blends petite + defined muscle instead of contradicting', () => {
    const s = specScene();
    s.characters[0]!.bodyType = 'petite';
    s.characters[0]!.muscle = 'defined';
    const text = compilePrompt(s);
    expect(text).toMatch(/petite athletic/i);
    expect(text).not.toMatch(/petite build.*athletic build/i);
  });

  it('forces ages to 18+', () => {
    const s = specScene();
    s.characters[0]!.age = '12';
    expect(sanitizeScene(s).characters[0]!.age).toBe('25');
  });

  it('keeps two characters distinct', () => {
    const s = specScene();
    const other = structuredClone(s.characters[0]!);
    other.id = 'b';
    other.gender = 'man';
    other.age = '32';
    other.hairColor = 'black';
    other.hairLength = 'short';
    other.eyeColor = 'brown';
    s.characters.push(other);
    s.relationship = 'couple';
    s.interaction = 'looking';
    const text = compilePrompt(s);
    expect(text).toMatch(/woman/i);
    expect(text).toMatch(/man/i);
    expect(text).toMatch(/On the left/i);
    expect(text).toMatch(/On the right/i);
  });

  it('makes minimal detail shorter than balanced', () => {
    const s = specScene();
    const balanced = compilePrompt(s);
    const minimal = compilePrompt({ ...s, detail: 'minimal' });
    expect(minimal.length).toBeLessThan(balanced.length);
  });

  it('compiles the cinematic preset', () => {
    const p = BUILTIN_PRESETS.find((x) => x.id === 'cinematic')!;
    const text = compilePrompt(p.scene);
    expect(text).toContain('woman');
    expect(looksLikeKeywordSalad(text)).toBe(false);
  });

  it('randomizes 18+ scenes that still compile', () => {
    for (let i = 0; i < 8; i++) {
      const s = randomizeScene(emptyScene());
      for (const ch of s.characters) {
        if (ch.age) expect(Number(ch.age)).toBeGreaterThanOrEqual(18);
      }
      expect(typeof compilePrompt(s)).toBe('string');
    }
  });

  it('compiles an empty scene to a short string', () => {
    expect(compilePrompt(emptyScene()).length).toBeLessThan(80);
  });
});
