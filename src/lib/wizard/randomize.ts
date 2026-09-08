import {
  ACCESSORIES,
  AGES,
  AMBIANCES,
  ANGLES,
  ARMS,
  BODY_TYPES,
  BOTTOMS,
  CAMERA_CHARS,
  COLOR_MOODS,
  COLORS,
  COMPOSITIONS,
  DRESSES,
  EYE_COLORS,
  EXPRESSIONS,
  FINISHES,
  FRAMINGS,
  GAZES,
  GENDERS,
  HAIR_COLORS,
  HAIR_LENGTHS,
  HAIR_TEXTURES,
  HAIRSTYLES,
  HEIGHTS,
  LIGHT_DIRS,
  LIGHT_EFFECTS,
  LIGHT_QUALITY,
  LIGHT_SOURCES,
  LIGHT_TEMP,
  LOCATIONS,
  MATERIALS,
  MOODS,
  NSFW_ATTIRE,
  OUTERWEAR,
  PHOTO_STYLES,
  POSITIONS,
  SHOES,
  SKIN_TONES,
  TIMES,
  TOPS,
  TORSOS,
  WEATHERS,
  pick,
  pickN,
  visibleOpts,
} from './catalog.ts';
import { emptyGarment } from './scene.ts';
import type { Character, Scene, SectionKey } from './types.ts';
import type { Opt } from './types.ts';

function sid(ids: string[]): string {
  return pick(ids);
}

function vis(list: Opt[], nsfw: boolean, tag?: string): Opt[] {
  let xs = visibleOpts(list, nsfw);
  if (tag) {
    const tagged = xs.filter((o) => o.tags?.includes(tag));
    if (tagged.length) xs = tagged;
  }
  return xs;
}

function idOf(list: Opt[], nsfw: boolean, tag?: string): string {
  return pick(vis(list, nsfw, tag)).id;
}

function maybe<T>(value: T, p = 0.55): T | undefined {
  return Math.random() < p ? value : undefined;
}

function locTags(scene: Scene): string[] {
  const loc = LOCATIONS.find((l) => l.id === scene.location);
  return loc?.tags ?? [];
}

function outfitFor(scene: Scene, ch: Character): Character {
  const tags = locTags(scene);
  const nsfw = scene.nsfw;
  const garments = [];

  if (nsfw && Math.random() < 0.45) {
    ch.nsfwAttire = pick(NSFW_ATTIRE).id;
    if (
      ch.nsfwAttire === 'nude' ||
      ch.nsfwAttire === 'artistic_nude' ||
      ch.nsfwAttire === 'explicit_nude'
    ) {
      if (Math.random() < 0.4) {
        garments.push({
          ...emptyGarment('accessory', pick(ACCESSORIES).id),
          color: pick(COLORS).id,
        });
      }
      ch.garments = garments;
      return ch;
    }
  } else {
    ch.nsfwAttire = undefined;
  }

  const warm = tags.includes('warm') || tags.includes('beach');
  const cold = tags.includes('cold') || scene.weather === 'snow';
  const office = tags.includes('office');
  const beach = tags.includes('beach');
  const formal = tags.includes('studio') && Math.random() < 0.3;

  if (beach) {
    garments.push({
      ...emptyGarment('top', 'tank'),
      color: pick(COLORS).id,
      fit: 'fitted',
    });
    garments.push({
      ...emptyGarment('bottom', 'shorts'),
      color: pick(COLORS).id,
    });
    garments.push({ ...emptyGarment('shoes', 'sandals') });
  } else if (Math.random() < 0.28) {
    const dress = pick(vis(DRESSES, nsfw, warm ? 'warm' : undefined));
    garments.push({
      ...emptyGarment('dress', dress.id),
      color: pick(COLORS).id,
      material: maybe(pick(MATERIALS).id, 0.5),
      fit: maybe('fitted', 0.4),
    });
    garments.push({
      ...emptyGarment('shoes', pick(vis(SHOES, nsfw, warm ? 'warm' : undefined)).id),
      color: sid(['black', 'brown', 'nude', 'white', 'tan']),
    });
  } else {
    const topPool = office
      ? TOPS.filter((t) => t.tags?.includes('smart') || t.id === 'shirt' || t.id === 'blouse')
      : vis(TOPS, nsfw, warm ? 'warm' : cold ? 'cold' : undefined);
    const botPool = office
      ? BOTTOMS.filter((t) => t.tags?.includes('smart') || t.id === 'trousers')
      : vis(BOTTOMS, nsfw, warm ? 'warm' : undefined);
    garments.push({
      ...emptyGarment('top', pick(topPool.length ? topPool : TOPS).id),
      color: pick(COLORS).id,
      material: maybe(pick(MATERIALS.filter((m) => !m.nsfw || nsfw)).id, 0.45),
      fit: maybe(pick(['fitted', 'relaxed', 'tailored']), 0.6),
    });
    garments.push({
      ...emptyGarment('bottom', pick(botPool.length ? botPool : BOTTOMS).id),
      color: pick(COLORS).id,
    });
    garments.push({
      ...emptyGarment('shoes', pick(vis(SHOES, nsfw)).id),
    });
  }

  if (cold) {
    garments.push({
      ...emptyGarment('outerwear', pick(OUTERWEAR).id),
      color: sid(['black', 'camel', 'navy', 'gray', 'brown']),
    });
  } else if (formal || office) {
    garments.push({
      ...emptyGarment('outerwear', 'blazer'),
      color: sid(['black', 'navy', 'gray', 'camel']),
    });
  }

  if (Math.random() < 0.55) {
    garments.push({
      ...emptyGarment('accessory', pick(visibleOpts(ACCESSORIES, nsfw)).id),
      color: sid(['silver', 'gold', 'black', 'brown']),
    });
  }

  ch.garments = garments;
  return ch;
}

function poseFor(scene: Scene, ch: Character): Character {
  const tags = locTags(scene);
  let pool = visibleOpts(POSITIONS, scene.nsfw);
  if (tags.includes('intimate') && scene.nsfw) {
    pool = POSITIONS.filter(
      (p) =>
        p.tags?.includes('intimate') ||
        p.tags?.includes('reclined') ||
        p.id === 'sitting' ||
        p.id === 'standing' ||
        p.id === 'leaning',
    );
  } else {
    pool = pool.filter((p) => !p.tags?.includes('intimate'));
  }
  ch.position = pick(pool).id;
  if (ch.position === 'running' || ch.position === 'walking') {
    ch.legsPose = 'walk_st';
  }
  if (
    ch.position === 'sitting' ||
    ch.position === 'reclining' ||
    ch.position === 'lying' ||
    ch.position === 'on_bed'
  ) {
    ch.legsPose = sid(['together', 'crossed', 'apart']);
  }
  ch.torso = maybe(idOf(TORSOS, scene.nsfw), 0.7);
  ch.arms = maybe(idOf(ARMS, scene.nsfw), 0.75);
  ch.gaze = idOf(GAZES, scene.nsfw);
  return ch;
}

function lightFor(scene: Scene): Scene {
  const tags = locTags(scene);
  const night =
    scene.time === 'night' ||
    scene.time === 'midnight' ||
    scene.time === 'blue' ||
    tags.includes('night');
  const sources = vis(
    LIGHT_SOURCES,
    scene.nsfw,
    night
      ? 'night'
      : tags.includes('studio')
        ? 'studio'
        : tags.includes('indoor')
          ? 'indoor'
          : 'day',
  );
  scene.lightSource = pick(sources.length ? sources : LIGHT_SOURCES).id;
  if (night && (scene.lightSource === 'sun' || scene.lightSource === 'natural')) {
    scene.lightSource = sid(['moon', 'neon', 'practical', 'candle']);
  }
  if (!night && scene.lightSource === 'moon') scene.lightSource = 'sun';
  scene.lightQuality = idOf(LIGHT_QUALITY, scene.nsfw);
  scene.lightTemp =
    scene.time === 'sunset' || scene.time === 'golden' || scene.lightSource === 'candle'
      ? 'warm'
      : scene.time === 'blue' || scene.time === 'night'
        ? 'cool'
        : idOf(LIGHT_TEMP, scene.nsfw);
  scene.lightDir = maybe(idOf(LIGHT_DIRS, scene.nsfw), 0.7);
  scene.lightEffects =
    maybe(
      pickN(LIGHT_EFFECTS, 1).map((x) => x.id),
      0.4,
    ) ?? [];
  return scene;
}

export function randomizeCharacter(scene: Scene, index = 0): Scene {
  if (scene.locks.character) return scene;
  const next = structuredClone(scene);
  const ch = next.characters[index] ?? next.characters[0];
  if (!ch) return next;
  if (!scene.locks.character) {
    ch.age = pick(AGES.filter((a) => Number(a.id) >= 21)).id;
    ch.gender = idOf(GENDERS, scene.nsfw);
    ch.eyeColor = idOf(EYE_COLORS, scene.nsfw);
    ch.skinTone = idOf(SKIN_TONES, scene.nsfw);
    ch.facialFeatures = [];
  }
  if (!scene.locks.body) {
    ch.bodyType = idOf(BODY_TYPES, scene.nsfw);
    ch.height = maybe(idOf(HEIGHTS, scene.nsfw), 0.5);
  }
  if (!scene.locks.hair) {
    ch.hairColor = idOf(HAIR_COLORS, scene.nsfw);
    ch.hairLength = idOf(HAIR_LENGTHS, scene.nsfw);
    ch.hairTexture = idOf(HAIR_TEXTURES, scene.nsfw);
    ch.hairstyle = maybe(idOf(HAIRSTYLES, scene.nsfw), 0.45);
  }
  if (!scene.locks.expression) {
    ch.expression = idOf(EXPRESSIONS, scene.nsfw);
  }
  next.characters[index] = ch;
  return next;
}

export function randomizeHair(scene: Scene, index = 0): Scene {
  if (scene.locks.hair) return scene;
  const next = structuredClone(scene);
  const ch = next.characters[index];
  if (!ch) return next;
  ch.hairColor = idOf(HAIR_COLORS, scene.nsfw);
  ch.hairLength = idOf(HAIR_LENGTHS, scene.nsfw);
  ch.hairTexture = idOf(HAIR_TEXTURES, scene.nsfw);
  ch.hairstyle = maybe(idOf(HAIRSTYLES, scene.nsfw), 0.5);
  ch.hairCustom = undefined;
  return next;
}

export function randomizeOutfit(scene: Scene, index = 0): Scene {
  if (scene.locks.outfit) return scene;
  const next = structuredClone(scene);
  const ch = next.characters[index];
  if (!ch) return next;
  next.characters[index] = outfitFor(next, ch);
  return next;
}

export function randomizePose(scene: Scene, index = 0): Scene {
  if (scene.locks.pose) return scene;
  const next = structuredClone(scene);
  const ch = next.characters[index];
  if (!ch) return next;
  next.characters[index] = poseFor(next, ch);
  return next;
}

export function randomizeEnvironment(scene: Scene): Scene {
  if (scene.locks.environment) return scene;
  const next = structuredClone(scene);
  next.location = idOf(LOCATIONS, scene.nsfw);
  next.ambiance = maybe(idOf(AMBIANCES, scene.nsfw), 0.6);
  next.time = idOf(TIMES, scene.nsfw);
  const tags = locTags(next);
  if (tags.includes('outdoor')) next.weather = maybe(idOf(WEATHERS, scene.nsfw), 0.55);
  else next.weather = undefined;
  if (tags.includes('beach')) {
    next.time = sid(['afternoon', 'golden', 'sunset']);
    next.weather = sid(['sunny', 'wind']);
  }
  return next;
}

export function randomizeLighting(scene: Scene): Scene {
  if (scene.locks.lighting) return scene;
  return lightFor(structuredClone(scene));
}

export function randomizeCamera(scene: Scene): Scene {
  if (scene.locks.camera) return scene;
  const next = structuredClone(scene);
  const moving = next.characters.some((c) => c.position === 'running' || c.position === 'walking');
  next.framing = moving ? sid(['full', 'wide', 'tq']) : idOf(FRAMINGS, scene.nsfw);
  next.lens =
    next.framing === 'full' || next.framing === 'wide'
      ? sid(['35', '50', '28'])
      : sid(['50', '85', '105']);
  next.dof =
    next.framing === 'wide' || next.framing === 'ewide'
      ? 'deep'
      : sid(['shallow', 'very_shallow', 'moderate']);
  next.angle = maybe(idOf(ANGLES, scene.nsfw), 0.6);
  next.cameraChar = maybe(idOf(CAMERA_CHARS, scene.nsfw), 0.3);
  return next;
}

export function randomizeStyle(scene: Scene): Scene {
  if (scene.locks.style) return scene;
  const next = structuredClone(scene);
  next.medium = sid(['photo', 'cinematic', 'photo', 'photo', 'illustration', 'anime']);
  next.photoStyle =
    next.medium === 'photo' || next.medium === 'cinematic'
      ? idOf(PHOTO_STYLES, scene.nsfw)
      : undefined;
  next.finish = idOf(FINISHES, scene.nsfw);
  next.colorMood = idOf(COLOR_MOODS, scene.nsfw);
  next.moods = pickN(visibleOpts(MOODS, scene.nsfw), 1 + (Math.random() < 0.35 ? 1 : 0)).map(
    (m) => m.id,
  );
  next.composition = maybe(idOf(COMPOSITIONS, scene.nsfw), 0.5);
  return next;
}

export function randomizeSection(scene: Scene, key: SectionKey, index = 0): Scene {
  switch (key) {
    case 'character':
    case 'body':
      return randomizeCharacter(scene, index);
    case 'hair':
      return randomizeHair(scene, index);
    case 'outfit':
      return randomizeOutfit(scene, index);
    case 'pose':
      return randomizePose(scene, index);
    case 'expression': {
      if (scene.locks.expression) return scene;
      const next = structuredClone(scene);
      const ch = next.characters[index];
      if (ch) ch.expression = idOf(EXPRESSIONS, scene.nsfw);
      return next;
    }
    case 'environment':
      return randomizeEnvironment(scene);
    case 'camera':
      return randomizeCamera(scene);
    case 'lighting':
      return randomizeLighting(scene);
    case 'style':
    case 'composition':
      return randomizeStyle(scene);
    default:
      return scene;
  }
}

export function randomizeScene(scene: Scene): Scene {
  let next = structuredClone(scene);
  next = randomizeEnvironment(next);
  next = randomizeCharacter(next, 0);
  next = randomizeHair(next, 0);
  next = randomizeOutfit(next, 0);
  next = randomizePose(next, 0);
  next = randomizeLighting(next);
  next = randomizeCamera(next);
  next = randomizeStyle(next);
  if (next.characters.length > 1) {
    for (let i = 1; i < next.characters.length; i++) {
      next = randomizeCharacter(next, i);
      next = randomizeHair(next, i);
      next = randomizeOutfit(next, i);
      next = randomizePose(next, i);
    }
  }
  return next;
}
