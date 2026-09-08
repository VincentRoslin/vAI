import {
  AMBIANCES,
  ANGLES,
  ARMS,
  BODY_TYPES,
  BUILDS,
  CAMERA_CHARS,
  COLOR_MOODS,
  COLORS,
  COMPOSITIONS,
  CONTRASTS,
  CURLS,
  DOFS,
  EYE_COLORS,
  EYE_EXPR,
  EYE_SHAPES,
  EXPRESSIONS,
  ERAS,
  FACE_SHAPES,
  FACIAL_FEATURES,
  FINISHES,
  FITS,
  FOCUSES,
  FRAMINGS,
  GAZES,
  HAIR_COLORS,
  HAIR_CUTS,
  HAIR_LENGTHS,
  HAIR_TEXTURES,
  HAIR_WET,
  HAIRSTYLES,
  BANGS,
  HEADS,
  HEIGHTS,
  HIGHLIGHTS,
  HIPS,
  INTERACTIONS,
  JAWS,
  LEG_POSES,
  LEGS,
  LENSES,
  LIGHT_DIRS,
  LIGHT_EFFECTS,
  LIGHT_INTENSITY,
  LIGHT_QUALITY,
  LIGHT_SOURCES,
  LIGHT_TEMP,
  LIPS,
  LOCATIONS,
  MARK_LOCATIONS,
  MATERIALS,
  MEDIUMS,
  MOODS,
  MOUTHS,
  MUSCLE,
  NECKLINES,
  NOSES,
  NSFW_ATTIRE,
  PARTINGS,
  PATTERNS,
  PHOTO_STYLES,
  POSITIONINGS,
  POSITIONS,
  PROP_POSITIONS,
  RELATIONSHIPS,
  SATURATIONS,
  SKIN_MARK_KINDS,
  SKIN_TEXTURES,
  SKIN_TONES,
  SLEEVES,
  SLOT_TYPES,
  STATES,
  SHOULDERS,
  SUBJECT_POS,
  TIMES,
  TORSOS,
  VOLUME,
  WAISTS,
  WEATHERS,
  BROWS,
  findOpt,
  promptOf,
} from './catalog.ts';
import { sanitizeScene } from './scene.ts';
import type { Character, CompileOptions, Garment, Scene } from './types.ts';

type Pronouns = {
  they: string;
  them: string;
  their: string;
  They: string;
  person: string;
  adult: string;
};

function pronouns(gender?: string): Pronouns {
  if (gender === 'woman') {
    return {
      they: 'she',
      them: 'her',
      their: 'her',
      They: 'She',
      person: 'woman',
      adult: 'adult woman',
    };
  }
  if (gender === 'man') {
    return { they: 'he', them: 'him', their: 'his', They: 'He', person: 'man', adult: 'adult man' };
  }
  if (gender === 'androgynous') {
    return {
      they: 'they',
      them: 'them',
      their: 'their',
      They: 'They',
      person: 'androgynous person',
      adult: 'androgynous adult',
    };
  }
  if (gender === 'nonbinary') {
    return {
      they: 'they',
      them: 'them',
      their: 'their',
      They: 'They',
      person: 'non-binary person',
      adult: 'non-binary adult',
    };
  }
  return {
    they: 'they',
    them: 'them',
    their: 'their',
    They: 'They',
    person: 'person',
    adult: 'adult',
  };
}

function her(text: string, p: Pronouns): string {
  return text
    .replace(/\bher\b/g, p.their)
    .replace(/\bshe\b/g, p.they)
    .replace(/\bShe\b/g, p.They);
}

function clean(parts: Array<string | undefined | null>): string[] {
  return parts.map((x) => (x ?? '').trim()).filter((x) => x.length > 0);
}

function andJoin(parts: Array<string | undefined | null>): string {
  const xs = clean(parts);
  if (xs.length === 0) return '';
  if (xs.length === 1) return xs[0]!;
  if (xs.length === 2) return `${xs[0]} and ${xs[1]}`;
  return `${xs.slice(0, -1).join(', ')}, and ${xs[xs.length - 1]}`;
}

function a(phrase: string): string {
  const p = phrase.trim();
  if (!p) return p;
  if (/^(a|an|the)\s/i.test(p)) return p;
  return /^[aeiou]/i.test(p) ? `an ${p}` : `a ${p}`;
}

function sentence(text: string): string {
  const t = text
    .trim()
    .replace(/\s+/g, ' ')
    .replace(/[ ,]+,/g, ',');
  if (!t) return '';
  const capped = t.charAt(0).toUpperCase() + t.slice(1);
  return /[.!?]$/.test(capped) ? capped : `${capped}.`;
}

function sentences(parts: Array<string | undefined | null>): string {
  return clean(parts.map((p) => (p ? sentence(p) : ''))).join(' ');
}

function agePhrase(ch: Character, p: Pronouns): string {
  if (ch.age) return `${ch.age}-year-old`;
  const map: Record<string, string> = {
    early_20s: `in ${p.their} early twenties`,
    mid_20s: `in ${p.their} mid-twenties`,
    late_20s: `in ${p.their} late twenties`,
    '30s': `in ${p.their} thirties`,
    '40s': `in ${p.their} forties`,
    '50s': `in ${p.their} fifties`,
    '60s': `in ${p.their} sixties`,
  };
  return ch.ageRange ? (map[ch.ageRange] ?? '') : '';
}

function describeHair(ch: Character, detail: Scene['detail']): string {
  if (ch.hairCustom?.trim()) return ch.hairCustom.trim();
  const color = promptOf(HAIR_COLORS, ch.hairColor);
  const length = promptOf(HAIR_LENGTHS, ch.hairLength);
  const texture = promptOf(HAIR_TEXTURES, ch.hairTexture);
  const curl = promptOf(CURLS, ch.curl);
  const cut = detail === 'detailed' ? promptOf(HAIR_CUTS, ch.hairCut) : '';
  const bangs = promptOf(BANGS, ch.bangs);
  const style = promptOf(HAIRSTYLES, ch.hairstyle);
  const hl = promptOf(HIGHLIGHTS, ch.highlights);
  const wet = ch.hairWet && ch.hairWet !== 'dry' ? promptOf(HAIR_WET, ch.hairWet) : '';
  const volume = detail === 'detailed' ? promptOf(VOLUME, ch.volume) : '';
  const parting = detail === 'detailed' ? promptOf(PARTINGS, ch.parting) : '';
  const textureBit = curl || texture;
  const core = clean([length, textureBit, color]).join(' ');
  if (!core && !style) return '';
  let phrase = core ? `${core} hair` : 'hair';
  const extras = clean([cut, volume, parting, bangs, hl]);
  if (extras.length && detail !== 'minimal') phrase += ` with ${andJoin(extras)}`;
  if (style && style !== 'worn down') phrase += ` ${style}`;
  if (wet) phrase += `, ${wet}`;
  return phrase;
}

function detailMuscle(m: string): boolean {
  return m.includes('defined') || m.includes('highly') || m.includes('light muscle');
}

function bodyBlend(ch: Character): string {
  const type = promptOf(BODY_TYPES, ch.bodyType);
  const build = promptOf(BUILDS, ch.build);
  const muscle = promptOf(MUSCLE, ch.muscle);
  const height = ch.height && ch.height !== 'average_h' ? promptOf(HEIGHTS, ch.height) : '';

  const blends: Record<string, string> = {
    'petite-defined': 'petite athletic build',
    'petite-ripped': 'petite muscular build',
    'slim-defined': 'slim athletic build',
    'lean-defined': 'lean athletic build',
    'curvy-defined': 'curvy athletic figure',
    'plus-defined': 'plus-size athletic build',
  };

  let core = blends[`${ch.bodyType}-${ch.muscle}`] ?? '';
  if (!core) {
    if (ch.bodyType === 'curvy') core = 'curvy figure';
    else if (ch.bodyType === 'plus') core = 'plus-size figure';
    else if (type) core = `${type} build`;
    else if (build) core = build;
  }
  if (muscle.includes('soft untoned') && core) core = `${core}, soft physique`;
  else if (
    muscle &&
    !core.includes('athletic') &&
    !core.includes('muscular') &&
    detailMuscle(muscle)
  ) {
    core = core ? `${core} with ${muscle}` : muscle;
  }
  return clean([height, core]).join(', ');
}

function describeFace(ch: Character, detail: Scene['detail']): string {
  if (detail === 'minimal') return '';
  const bits = clean([
    promptOf(FACE_SHAPES, ch.faceShape),
    promptOf(JAWS, ch.jaw),
    detail === 'detailed' ? promptOf(NOSES, ch.nose) : '',
    promptOf(LIPS, ch.lips),
    promptOf(EYE_SHAPES, ch.eyeShape),
    promptOf(BROWS, ch.brows),
    ...ch.facialFeatures.map((id) => promptOf(FACIAL_FEATURES, id)),
  ]);
  if (detail === 'balanced') return andJoin(bits.slice(0, 2));
  return andJoin(bits);
}

function describeSkin(ch: Character, detail: Scene['detail']): string {
  const tone = promptOf(SKIN_TONES, ch.skinTone);
  const tex = detail === 'detailed' ? promptOf(SKIN_TEXTURES, ch.skinTexture) : '';
  const marks = ch.skinMarks
    .map((m) => {
      const kind = promptOf(SKIN_MARK_KINDS, m.kind);
      const loc = promptOf(MARK_LOCATIONS, m.location);
      if (!kind) return '';
      return loc ? `${kind} ${loc}` : kind;
    })
    .filter(Boolean);
  if (detail === 'minimal' && marks.length === 0) return '';
  return andJoin([tone, tex, ...marks]);
}

function garmentPhrase(g: Garment): string {
  if (g.custom?.trim()) return g.custom.trim();
  const types = SLOT_TYPES[g.slot] ?? [];
  const type = types.find((t) => t.id === g.typeId)?.prompt || g.typeId;
  if (!type) return '';
  const bits = clean([
    promptOf(STATES, g.state),
    promptOf(FITS, g.fit),
    promptOf(COLORS, g.color),
    promptOf(MATERIALS, g.material),
    promptOf(PATTERNS, g.pattern),
    g.slot === 'top' || g.slot === 'dress' ? promptOf(NECKLINES, g.neckline) : '',
    g.slot === 'top' ? promptOf(SLEEVES, g.sleeve) : '',
    type,
  ]);
  return bits.join(' ');
}

const NUDE_IDS = new Set(['nude', 'artistic_nude', 'implied_nude', 'explicit_nude']);

function describeOutfit(ch: Character, nsfw: boolean, detail: Scene['detail']): string {
  if (ch.outfitCustom?.trim()) return ch.outfitCustom.trim();
  const attire = nsfw ? promptOf(NSFW_ATTIRE, ch.nsfwAttire) : '';
  const isNude = nsfw && ch.nsfwAttire && NUDE_IDS.has(ch.nsfwAttire);
  const garments = ch.garments.filter((g) => g.typeId || g.custom);
  const hasDress = garments.some((g) => g.slot === 'dress');
  const filtered = garments.filter((g) => {
    if (isNude && g.slot !== 'accessory' && g.slot !== 'shoes') return false;
    if (hasDress && (g.slot === 'top' || g.slot === 'bottom')) return false;
    return true;
  });
  const clothes = filtered
    .filter((g) => g.slot !== 'accessory')
    .map(garmentPhrase)
    .filter(Boolean);
  const accessories = filtered
    .filter((g) => g.slot === 'accessory')
    .map(garmentPhrase)
    .filter(Boolean);

  if (isNude) {
    const jewel = accessories.length ? `except for ${andJoin(accessories)}` : '';
    const base = attire || 'nude';
    return jewel ? `${base} ${jewel}` : base;
  }
  if (attire && nsfw) {
    const extra = andJoin([...clothes, ...accessories]);
    return extra ? `wearing ${attire}, with ${extra}` : `wearing ${attire}`;
  }
  const all =
    detail === 'minimal' ? [...clothes, ...accessories].slice(0, 3) : [...clothes, ...accessories];
  if (!all.length) return '';
  const dressed = all.map((item, i) => (i === 0 ? a(item) : item));
  return `wearing ${andJoin(dressed)}`;
}

function indoorHome(location?: string): boolean {
  return location === 'bedroom' || location === 'apartment' || location === 'hotel';
}

function describePose(ch: Character, p: Pronouns, scene: Scene): string {
  if (ch.poseCustom?.trim()) return ch.poseCustom.trim();
  const pos = findOpt(POSITIONS, ch.position);
  const verb = pos?.prompt ?? '';
  const torso = promptOf(TORSOS, ch.torso);
  const head = promptOf(HEADS, ch.head);
  const arms = her(promptOf(ARMS, ch.arms), p);
  const legs = promptOf(LEG_POSES, ch.legsPose);
  const gaze = her(promptOf(GAZES, ch.gaze), p);
  if (!verb && !gaze && !arms) return '';

  let action = verb ? `${p.They} ${verb}` : p.They;
  const loc = promptOf(LOCATIONS, scene.location);
  if (
    verb &&
    indoorHome(scene.location) &&
    (ch.position === 'standing' || ch.position === 'leaning')
  ) {
    action = `${p.They} ${verb} beside a ${loc.replace(/^an?\s/, '')} window`;
  } else if (verb && ch.position === 'sitting' && loc) {
    action = `${p.They} ${verb} in ${loc}`;
  } else if (verb && loc && ch.position !== 'on_bed') {
    action = `${p.They} ${verb} in ${loc}`;
  }

  const extras = clean([
    torso && torso !== 'facing the camera' ? torso : '',
    head && head !== 'head level' ? head : '',
    arms,
    scene.detail === 'detailed' ? legs : '',
  ]);
  if (extras[0]) action += `, ${extras[0]}`;
  if (extras[1]) action += `, ${extras[1]}`;
  if (gaze) {
    action = action === p.They ? `${p.They} is ${gaze}` : `${action}, ${gaze}`;
  }
  return action;
}

function describeExpression(ch: Character): string {
  if (ch.expressionCustom?.trim()) return ch.expressionCustom.trim();
  const expr = promptOf(EXPRESSIONS, ch.expression);
  const mouth = promptOf(MOUTHS, ch.mouth);
  const eyes = promptOf(EYE_EXPR, ch.eyes);
  if (expr && !mouth && !eyes) return expr;
  return andJoin([expr, mouth, eyes]);
}

function attachExpression(action: string, expr: string, p: Pronouns): string {
  if (!expr) return action;
  if (!action) return `${p.They} has ${expr}`;
  const smile = expr;
  if (/smile|laugh|expression|look|gaze|flirty|seductive/i.test(expr)) {
    return `${action} with ${smile}`;
  }
  return `${action}, ${expr}`;
}

function placeNoun(scene: Scene): string {
  if (scene.locationCustom?.trim()) return scene.locationCustom.trim();
  return promptOf(LOCATIONS, scene.location);
}

function describePlace(scene: Scene): string {
  const loc = placeNoun(scene);
  if (!loc) return '';
  const amb = promptOf(AMBIANCES, scene.ambiance);
  const time = promptOf(TIMES, scene.time);
  const weather = promptOf(WEATHERS, scene.weather);
  let phrase = clean([amb, loc]).join(' ');
  if (time) phrase += ` at ${time}`;
  if (weather) phrase += `, ${weather}`;
  return phrase;
}

function describeLight(scene: Scene, windowStory: boolean): string {
  if (windowStory && (scene.time === 'sunset' || scene.time === 'golden')) {
    const effects = scene.lightEffects.map((id) => promptOf(LIGHT_EFFECTS, id)).filter(Boolean);
    let s =
      'Warm evening light enters through the window, creating soft shadows and gentle highlights in the hair';
    if (effects.length) s += `, ${andJoin(effects)}`;
    return s;
  }
  const src = promptOf(LIGHT_SOURCES, scene.lightSource);
  const dir = promptOf(LIGHT_DIRS, scene.lightDir);
  const qual = promptOf(LIGHT_QUALITY, scene.lightQuality);
  const temp = promptOf(LIGHT_TEMP, scene.lightTemp);
  const intensity = promptOf(LIGHT_INTENSITY, scene.lightIntensity);
  const effects = scene.lightEffects.map((id) => promptOf(LIGHT_EFFECTS, id)).filter(Boolean);
  const qualTemp = clean([
    intensity && intensity !== 'moderate' ? intensity : '',
    qual,
    temp,
    src,
  ]).join(' ');
  if (!qualTemp && !dir && !effects.length) return '';
  let s = qualTemp || 'light';
  if (dir) s += ` from the ${dir.replace(' light', '')}`;
  if (effects.length) s += `, with ${andJoin(effects)}`;
  return s;
}

function describeStyle(scene: Scene): string {
  const medium = promptOf(MEDIUMS, scene.medium);
  const photo = promptOf(PHOTO_STYLES, scene.photoStyle);
  const era = promptOf(ERAS, scene.era);
  const finish = promptOf(FINISHES, scene.finish);
  const framing = promptOf(FRAMINGS, scene.framing);
  const lens = promptOf(LENSES, scene.lens);
  const dof = promptOf(DOFS, scene.dof);
  const focus = promptOf(FOCUSES, scene.focus);
  const angle = promptOf(ANGLES, scene.angle);
  const cam = promptOf(CAMERA_CHARS, scene.cameraChar);
  const comp = promptOf(COMPOSITIONS, scene.composition);
  const subPos = promptOf(SUBJECT_POS, scene.subjectPos);
  const color = promptOf(COLOR_MOODS, scene.colorMood);
  const sat = promptOf(SATURATIONS, scene.saturation);
  const contrast = promptOf(CONTRASTS, scene.contrast);
  const moods = scene.moods.map((id) => promptOf(MOODS, id)).filter(Boolean);

  const styleBits = clean([
    photo,
    medium && medium !== 'photograph' && !photo ? medium : !photo ? medium : '',
    era && era !== 'contemporary' ? era : '',
    finish,
  ]);
  const camBits = clean([
    framing,
    angle && angle !== 'eye-level' ? angle : '',
    lens,
    dof,
    scene.detail === 'detailed' ? focus : '',
    scene.detail === 'detailed' ? cam : '',
    scene.detail === 'detailed' ? comp : '',
    scene.detail === 'detailed' ? subPos : '',
  ]);
  const colorBits = clean([
    color,
    scene.detail === 'detailed' ? sat : '',
    scene.detail === 'detailed' ? contrast : '',
    scene.dominantColor ? `${promptOf(COLORS, scene.dominantColor)} dominant` : '',
    scene.accentColor ? `${promptOf(COLORS, scene.accentColor)} accents` : '',
  ]);
  const skinBit =
    scene.medium === 'photo' || scene.medium === 'cinematic' || !scene.medium
      ? 'natural skin texture'
      : '';
  return clean([
    styleBits.join(' '),
    skinBit,
    camBits.join(', '),
    colorBits.join(', '),
    moods.length ? moods.join(' and ') : '',
  ]).join(', ');
}

function describeProps(scene: Scene): string {
  if (!scene.props.length) return '';
  const bits = scene.props.map((p) => {
    const name = p.name.trim() || p.type || '';
    if (!name) return '';
    const obj = clean([promptOf(COLORS, p.color), promptOf(MATERIALS, p.material), name]).join(' ');
    const rest = clean([
      promptOf(PROP_POSITIONS, p.position),
      p.state?.trim(),
      p.interaction?.trim(),
    ]);
    return rest.length ? `${obj} ${rest.join(', ')}` : obj;
  });
  return andJoin(bits);
}

function opening(
  ch: Character,
  p: Pronouns,
  nsfw: boolean,
  voice: 0 | 1 | 2,
  lightLead?: string,
): string {
  const age = agePhrase(ch, p);
  const who = nsfw && !ch.age && !ch.ageRange ? p.adult : p.person;
  const hair = describeHair(ch, 'balanced');
  const eyes = promptOf(EYE_COLORS, ch.eyeColor);
  const body = bodyBlend(ch);
  let head = '';
  if (age && !age.startsWith('in ')) head = `${a(age)} ${who}`;
  else if (age) head = `${a(who)} ${age}`;
  else head = nsfw ? a(p.adult) : a(who);
  const withBits = clean([hair, eyes]);
  if (withBits.length) head += ` with ${andJoin(withBits)}`;
  if (body) head += `, ${body}`;
  if (voice === 1 && lightLead) return `${lightLead}, ${head}`;
  if (voice === 2) return `Portrait of ${head}`;
  return head;
}

function characterBlock(ch: Character, scene: Scene, voice: 0 | 1 | 2): string[] {
  const p = pronouns(ch.gender);
  const windowStory =
    indoorHome(scene.location) &&
    (scene.lightSource === 'window' || scene.time === 'sunset' || scene.time === 'golden');
  const lightLead =
    voice === 1
      ? describeLight(scene, windowStory) ||
        (promptOf(TIMES, scene.time) ? `At ${promptOf(TIMES, scene.time)}` : '')
      : '';
  const head = opening(ch, p, scene.nsfw, voice, lightLead);
  const face = describeFace(ch, scene.detail);
  const skin = describeSkin(ch, scene.detail);
  const outfit = describeOutfit(ch, scene.nsfw, scene.detail);
  let intro = head;
  if (outfit) intro += `, ${outfit}`;
  if (face && scene.detail !== 'minimal') intro += `, ${face}`;
  if (skin && scene.detail === 'detailed') intro += `, ${skin}`;
  if (scene.detail === 'detailed') {
    const proportions = andJoin([
      promptOf(SHOULDERS, ch.shoulders),
      promptOf(WAISTS, ch.waist),
      promptOf(HIPS, ch.hips),
      promptOf(LEGS, ch.legs),
    ]);
    if (proportions) intro += `, ${proportions}`;
  }
  const pose = describePose(ch, p, scene);
  const expr = describeExpression(ch);
  const action = attachExpression(pose, expr, p);
  const out = [intro];
  if (action && scene.detail !== 'minimal') out.push(action);
  return out;
}

function compileKrea2(sceneIn: Scene, options: CompileOptions = {}): string {
  const scene = sanitizeScene(sceneIn);
  const voice = options.voice ?? 0;
  const chars = scene.characters;
  const loc = placeNoun(scene);
  const place = describePlace(scene);
  const windowStory =
    indoorHome(scene.location) &&
    (scene.lightSource === 'window' || scene.time === 'sunset' || scene.time === 'golden');
  const parts: string[] = [];

  if (chars.length >= 2) {
    const people = chars.map((c) => pronouns(c.gender).person);
    const rel = promptOf(RELATIONSHIPS, scene.relationship);
    const inter = promptOf(INTERACTIONS, scene.interaction);
    const pos = promptOf(POSITIONINGS, scene.positioning);
    const count =
      chars.length === 2 ? `Two people, ${people[0]} and ${people[1]}` : `${chars.length} people`;
    parts.push(andJoin([count, rel, pos, inter]));
    chars.forEach((ch, i) => {
      const side = i === 0 ? 'On the left' : i === 1 ? 'On the right' : 'Nearby';
      const block = characterBlock(ch, scene, 0);
      parts.push(`${side}, ${block[0]}`);
      if (block[1]) parts.push(block[1]);
    });
  } else if (chars[0]) {
    parts.push(...characterBlock(chars[0], scene, voice));
  }

  const alreadyPlaced =
    loc && parts.some((x) => x.toLowerCase().includes(loc.replace(/^an?\s/, '').toLowerCase()));
  if (scene.detail !== 'minimal') {
    const light = describeLight(scene, windowStory && chars.length === 1);
    if (light && !(voice === 1 && chars.length === 1)) {
      if (windowStory) parts.push(light);
      else if (place && !alreadyPlaced) parts.push(`Set in ${place}, ${light}`);
      else if (light) parts.push(light);
    } else if (place && !alreadyPlaced) {
      parts.push(`Set in ${place}`);
    }
  } else if (place && !alreadyPlaced) {
    const last = parts[0] ?? '';
    parts[0] = last ? `${last}, in ${place}` : `In ${place}`;
  }

  const props = describeProps(scene);
  if (props) parts.push(`${props} in the scene`);
  const style = describeStyle(scene);
  if (style) parts.push(style);
  if (scene.additional?.trim()) parts.push(scene.additional.trim());

  return sentences(parts)
    .replace(/\s+/g, ' ')
    .replace(/\s,/g, ',')
    .replace(/,\s*,/g, ',')
    .replace(/\s+\./g, '.')
    .replace(/\.\./g, '.')
    .trim();
}

export function compilePrompt(scene: Scene, options: CompileOptions = {}): string {
  return compileKrea2(scene, options);
}

export function compileShort(scene: Scene): string {
  return compileKrea2({ ...scene, detail: 'minimal' });
}

export function compileLong(scene: Scene): string {
  return compileKrea2({ ...scene, detail: 'detailed' });
}

export function rewritePrompt(
  scene: Scene,
  currentVoice: 0 | 1 | 2 = 0,
): { text: string; voice: 0 | 1 | 2 } {
  const next = ((currentVoice + 1) % 3) as 0 | 1 | 2;
  return { text: compileKrea2(scene, { voice: next }), voice: next };
}

export function looksLikeKeywordSalad(text: string): boolean {
  if (!text) return false;
  const commas = (text.match(/,/g) ?? []).length;
  const sentencesN = (text.match(/[.!?]/g) ?? []).length;
  return commas > 8 && sentencesN < 2;
}
