import { uid } from '../id.ts';
import type { Character, Garment, PropItem, Scene, SkinMark } from './types';

export function emptyCharacter(): Character {
  return {
    id: uid(),
    facialFeatures: [],
    skinMarks: [],
    garments: [],
  };
}

export function emptyScene(): Scene {
  return {
    nsfw: false,
    detail: 'balanced',
    characters: [emptyCharacter()],
    lightEffects: [],
    moods: [],
    props: [],
    locks: {},
  };
}

export function emptyGarment(slot: Garment['slot'], typeId = ''): Garment {
  return { id: uid(), slot, typeId };
}

export function emptyMark(kind: string): SkinMark {
  return { id: uid(), kind };
}

export function emptyProp(name = ''): PropItem {
  return { id: uid(), name };
}

export function cloneScene(scene: Scene): Scene {
  return structuredClone(scene);
}

export function sceneHasContent(scene: Scene): boolean {
  const c = scene.characters[0];
  if (!c) {
    return Boolean(scene.location || scene.medium || scene.props.length || scene.additional);
  }
  return Boolean(
    c.age ||
    c.gender ||
    c.hairColor ||
    c.bodyType ||
    c.garments.length ||
    c.nsfwAttire ||
    c.position ||
    c.expression ||
    scene.location ||
    scene.medium ||
    scene.additional,
  );
}

/** Force 18+ language. Never emit a minor. */
export function sanitizeScene(scene: Scene): Scene {
  const next = cloneScene(scene);
  for (const ch of next.characters) {
    if (ch.age) {
      const n = Number.parseInt(ch.age, 10);
      if (Number.isFinite(n) && n < 18) ch.age = '25';
    }
  }
  if (!next.nsfw) {
    for (const ch of next.characters) {
      ch.nsfwAttire = undefined;
    }
    next.moods = next.moods.filter((m) => m !== 'sensual' && m !== 'erotic');
  }
  return next;
}
