import { emptyGarment, emptyScene } from './scene.ts';
import type { Scene } from './types.ts';

function base(): Scene {
  const s = emptyScene();
  const ch = s.characters[0]!;
  ch.age = '25';
  ch.gender = 'woman';
  return s;
}

export type BuiltinPreset = {
  id: string;
  name: string;
  hint: string;
  nsfw?: boolean;
  scene: Scene;
};

function cinematicPortrait(): Scene {
  const s = base();
  const ch = s.characters[0]!;
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
  s.ambiance = 'cozy';
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

function fashionEditorial(): Scene {
  const s = base();
  const ch = s.characters[0]!;
  ch.age = '28';
  ch.hairColor = 'black';
  ch.hairLength = 'long';
  ch.hairTexture = 'straight';
  ch.eyeColor = 'brown';
  ch.bodyType = 'athletic';
  ch.garments = [
    { ...emptyGarment('dress', 'slip'), color: 'black', material: 'silk', fit: 'fitted' },
    { ...emptyGarment('shoes', 'heels'), color: 'black' },
    { ...emptyGarment('accessory', 'earrings'), color: 'gold' },
  ];
  ch.position = 'standing';
  ch.torso = 'three_q';
  ch.arms = 'hip';
  ch.gaze = 'away';
  ch.expression = 'serious';
  s.location = 'studio';
  s.ambiance = 'minimal';
  s.lightSource = 'studio';
  s.lightQuality = 'hard';
  s.lightDir = 'side';
  s.framing = 'full';
  s.lens = '85';
  s.dof = 'moderate';
  s.medium = 'photo';
  s.photoStyle = 'editorial';
  s.finish = 'editorial_f';
  s.colorMood = 'neutral_c';
  s.moods = ['dramatic'];
  return s;
}

function casualLifestyle(): Scene {
  const s = base();
  const ch = s.characters[0]!;
  ch.age = '30';
  ch.gender = 'woman';
  ch.hairColor = 'brown';
  ch.hairLength = 'shoulder';
  ch.hairTexture = 'wavy';
  ch.eyeColor = 'hazel';
  ch.bodyType = 'average';
  ch.garments = [
    { ...emptyGarment('top', 'sweater'), color: 'cream', material: 'knit' },
    { ...emptyGarment('bottom', 'jeans'), color: 'blue' },
    { ...emptyGarment('shoes', 'sneakers'), color: 'white' },
  ];
  ch.position = 'sitting';
  ch.gaze = 'camera';
  ch.expression = 'smiling';
  s.location = 'cafe';
  s.ambiance = 'cozy';
  s.time = 'morning';
  s.lightSource = 'window';
  s.lightQuality = 'soft';
  s.framing = 'half';
  s.lens = '50';
  s.dof = 'shallow';
  s.medium = 'photo';
  s.photoStyle = 'doc';
  s.finish = 'natural_f';
  s.colorMood = 'warm_c';
  s.moods = ['peaceful'];
  s.props = [{ id: 'p1', name: 'coffee cup', position: 'hand' }];
  return s;
}

function streetPhotography(): Scene {
  const s = base();
  const ch = s.characters[0]!;
  ch.age = '32';
  ch.gender = 'man';
  ch.hairColor = 'black';
  ch.hairLength = 'short';
  ch.hairTexture = 'straight';
  ch.eyeColor = 'brown';
  ch.bodyType = 'lean';
  ch.garments = [
    { ...emptyGarment('top', 'tshirt'), color: 'white' },
    { ...emptyGarment('outerwear', 'leather_j'), color: 'black' },
    { ...emptyGarment('bottom', 'jeans'), color: 'black' },
    { ...emptyGarment('shoes', 'boots'), color: 'black' },
  ];
  ch.position = 'walking';
  ch.gaze = 'away';
  ch.expression = 'neutral';
  s.location = 'street';
  s.time = 'golden';
  s.weather = 'cloudy';
  s.lightSource = 'sun';
  s.lightQuality = 'hard';
  s.framing = 'full';
  s.lens = '35';
  s.dof = 'deep';
  s.cameraChar = 'handheld';
  s.medium = 'photo';
  s.photoStyle = 'street';
  s.finish = 'gritty';
  s.colorMood = 'muted';
  s.moods = ['energetic'];
  return s;
}

function studioPortrait(): Scene {
  const s = base();
  const ch = s.characters[0]!;
  ch.age = '35';
  ch.hairColor = 'auburn';
  ch.hairLength = 'medium';
  ch.hairTexture = 'wavy';
  ch.eyeColor = 'green';
  ch.bodyType = 'slim';
  ch.skinTone = 'fair';
  ch.garments = [{ ...emptyGarment('top', 'blouse'), color: 'black', material: 'silk' }];
  ch.position = 'sitting';
  ch.gaze = 'camera';
  ch.expression = 'thoughtful';
  s.location = 'studio';
  s.ambiance = 'clean';
  s.lightSource = 'studio';
  s.lightQuality = 'soft';
  s.lightDir = 'side';
  s.framing = 'head';
  s.lens = '85';
  s.dof = 'very_shallow';
  s.focus = 'eyes';
  s.medium = 'photo';
  s.photoStyle = 'portrait';
  s.finish = 'photoreal';
  s.colorMood = 'neutral_c';
  s.moods = ['intimate'];
  return s;
}

function characterConcept(): Scene {
  const s = base();
  const ch = s.characters[0]!;
  ch.age = '27';
  ch.hairColor = 'red';
  ch.hairLength = 'long';
  ch.hairTexture = 'wavy';
  ch.eyeColor = 'amber';
  ch.bodyType = 'athletic';
  ch.garments = [
    { ...emptyGarment('outerwear', 'coat'), color: 'charcoal' },
    { ...emptyGarment('top', 'tshirt'), color: 'black' },
    { ...emptyGarment('bottom', 'trousers'), color: 'black' },
  ];
  ch.position = 'standing';
  ch.torso = 'three_q';
  ch.expression = 'confident';
  ch.gaze = 'camera';
  s.location = 'warehouse';
  s.ambiance = 'industrial';
  s.time = 'night';
  s.lightSource = 'practical';
  s.lightQuality = 'dramatic';
  s.framing = 'tq';
  s.lens = '50';
  s.medium = 'digital';
  s.finish = 'cinematic_f';
  s.colorMood = 'cool_c';
  s.moods = ['dramatic'];
  return s;
}

function animeCharacter(): Scene {
  const s = base();
  const ch = s.characters[0]!;
  ch.age = '21';
  ch.hairColor = 'black';
  ch.hairLength = 'long';
  ch.hairTexture = 'straight';
  ch.eyeColor = 'blue';
  ch.bodyType = 'slim';
  ch.garments = [
    { ...emptyGarment('top', 'sweater'), color: 'cream' },
    { ...emptyGarment('bottom', 'skirt'), color: 'navy' },
  ];
  ch.position = 'standing';
  ch.gaze = 'camera';
  ch.expression = 'slight_smile';
  s.location = 'park';
  s.time = 'afternoon';
  s.weather = 'sunny';
  s.lightSource = 'natural';
  s.framing = 'half';
  s.medium = 'anime';
  s.finish = 'polished';
  s.colorMood = 'pastel';
  s.moods = ['peaceful'];
  return s;
}

function productPhotography(): Scene {
  const s = emptyScene();
  s.characters = [];
  s.location = 'studio';
  s.ambiance = 'minimal';
  s.lightSource = 'studio';
  s.lightQuality = 'soft';
  s.framing = 'cu';
  s.lens = '105';
  s.dof = 'shallow';
  s.medium = 'photo';
  s.finish = 'polished';
  s.colorMood = 'neutral_c';
  s.composition = 'centered';
  s.props = [{ id: 'p1', name: 'the product', position: 'center', state: 'clean, hero angle' }];
  s.additional = 'Clean catalog product shot, accurate materials, no people.';
  return s;
}

function boudoir(): Scene {
  const s = base();
  s.nsfw = true;
  const ch = s.characters[0]!;
  ch.age = '28';
  ch.hairColor = 'dark_brown';
  ch.hairLength = 'long';
  ch.hairTexture = 'wavy';
  ch.eyeColor = 'brown';
  ch.bodyType = 'curvy';
  ch.nsfwAttire = 'lace_lingerie';
  ch.position = 'on_bed';
  ch.gaze = 'camera';
  ch.expression = 'seductive';
  s.location = 'bedroom';
  s.ambiance = 'luxury';
  s.time = 'night';
  s.lightSource = 'window';
  s.lightQuality = 'soft';
  s.lightTemp = 'warm';
  s.framing = 'tq';
  s.lens = '85';
  s.dof = 'shallow';
  s.medium = 'photo';
  s.photoStyle = 'editorial';
  s.finish = 'cinematic_f';
  s.colorMood = 'muted_warm';
  s.moods = ['sensual', 'intimate'];
  return s;
}

function artisticNude(): Scene {
  const s = base();
  s.nsfw = true;
  const ch = s.characters[0]!;
  ch.age = '30';
  ch.hairColor = 'brown';
  ch.hairLength = 'medium';
  ch.hairTexture = 'wavy';
  ch.eyeColor = 'hazel';
  ch.bodyType = 'lean';
  ch.nsfwAttire = 'artistic_nude';
  ch.position = 'sitting';
  ch.torso = 'three_q';
  ch.gaze = 'away';
  ch.expression = 'thoughtful';
  s.location = 'studio';
  s.ambiance = 'minimal';
  s.lightSource = 'studio';
  s.lightQuality = 'soft';
  s.lightDir = 'side';
  s.framing = 'tq';
  s.lens = '85';
  s.dof = 'shallow';
  s.medium = 'photo';
  s.photoStyle = 'portrait';
  s.finish = 'natural_f';
  s.colorMood = 'muted';
  s.moods = ['intimate'];
  return s;
}

export const BUILTIN_PRESETS: BuiltinPreset[] = [
  {
    id: 'cinematic',
    name: 'Cinematic portrait',
    hint: 'Bedroom window, 85mm, warm dusk',
    scene: cinematicPortrait(),
  },
  {
    id: 'editorial',
    name: 'Fashion editorial',
    hint: 'Studio, silk, hard side light',
    scene: fashionEditorial(),
  },
  {
    id: 'lifestyle',
    name: 'Casual lifestyle',
    hint: 'Café, knit, morning window',
    scene: casualLifestyle(),
  },
  {
    id: 'street',
    name: 'Street photography',
    hint: 'City walk, 35mm, golden hour',
    scene: streetPhotography(),
  },
  {
    id: 'studio',
    name: 'Studio portrait',
    hint: 'Headshot, softbox, eyes sharp',
    scene: studioPortrait(),
  },
  {
    id: 'concept',
    name: 'Character concept',
    hint: 'Industrial night, cinematic',
    scene: characterConcept(),
  },
  {
    id: 'anime',
    name: 'Anime character',
    hint: 'Park afternoon, clean illustration',
    scene: animeCharacter(),
  },
  {
    id: 'product',
    name: 'Product photography',
    hint: 'Studio hero shot, no people',
    scene: productPhotography(),
  },
  {
    id: 'boudoir',
    name: 'Boudoir',
    hint: 'Adult, lingerie, hotel light',
    scene: boudoir(),
    nsfw: true,
  },
  {
    id: 'nude',
    name: 'Artistic nude',
    hint: 'Adult, studio, tasteful nude',
    scene: artisticNude(),
    nsfw: true,
  },
];
