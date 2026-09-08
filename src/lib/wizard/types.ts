export type DetailLevel = 'minimal' | 'balanced' | 'detailed';

export type GarmentSlot =
  'top' | 'bottom' | 'dress' | 'outerwear' | 'shoes' | 'accessory' | 'custom';

export type Opt = {
  id: string;
  label: string;
  prompt: string;
  nsfw?: boolean;
  tags?: string[];
};

export type Garment = {
  id: string;
  slot: GarmentSlot;
  typeId: string;
  color?: string;
  material?: string;
  pattern?: string;
  fit?: string;
  neckline?: string;
  sleeve?: string;
  length?: string;
  state?: string;
  custom?: string;
};

export type SkinMark = {
  id: string;
  kind: string;
  location?: string;
  size?: string;
  visibility?: string;
};

export type PropItem = {
  id: string;
  name: string;
  type?: string;
  color?: string;
  material?: string;
  position?: string;
  state?: string;
  interaction?: string;
};

export type Character = {
  id: string;
  /** Display name for multi-character prompts */
  name?: string;
  age?: string;
  ageRange?: string;
  gender?: string;
  faceShape?: string;
  jaw?: string;
  nose?: string;
  lips?: string;
  eyeColor?: string;
  eyeShape?: string;
  brows?: string;
  facialFeatures: string[];
  skinTone?: string;
  skinTexture?: string;
  skinMarks: SkinMark[];
  height?: string;
  bodyType?: string;
  build?: string;
  muscle?: string;
  shoulders?: string;
  waist?: string;
  hips?: string;
  legs?: string;
  hairColor?: string;
  hairLength?: string;
  hairTexture?: string;
  curl?: string;
  hairCut?: string;
  bangs?: string;
  parting?: string;
  volume?: string;
  hairstyle?: string;
  highlights?: string;
  hairWet?: string;
  hairCustom?: string;
  garments: Garment[];
  outfitCustom?: string;
  nsfwAttire?: string;
  position?: string;
  torso?: string;
  head?: string;
  arms?: string;
  hands?: string;
  legsPose?: string;
  gaze?: string;
  poseCustom?: string;
  expression?: string;
  mouth?: string;
  eyes?: string;
  expressionCustom?: string;
};

export type SectionKey =
  | 'character'
  | 'hair'
  | 'body'
  | 'outfit'
  | 'pose'
  | 'expression'
  | 'environment'
  | 'camera'
  | 'lighting'
  | 'composition'
  | 'style'
  | 'props'
  | 'extra';

export type Scene = {
  nsfw: boolean;
  detail: DetailLevel;
  characters: Character[];
  relationship?: string;
  interaction?: string;
  positioning?: string;
  location?: string;
  locationCustom?: string;
  ambiance?: string;
  time?: string;
  weather?: string;
  framing?: string;
  angle?: string;
  lens?: string;
  dof?: string;
  focus?: string;
  cameraChar?: string;
  lightSource?: string;
  lightDir?: string;
  lightQuality?: string;
  lightTemp?: string;
  lightIntensity?: string;
  lightEffects: string[];
  composition?: string;
  subjectPos?: string;
  medium?: string;
  photoStyle?: string;
  era?: string;
  finish?: string;
  colorMood?: string;
  dominantColor?: string;
  accentColor?: string;
  saturation?: string;
  contrast?: string;
  moods: string[];
  props: PropItem[];
  additional?: string;
  locks: Partial<Record<SectionKey, boolean>>;
};

export type SavedPreset = {
  id: string;
  name: string;
  scene: Scene;
  createdAt: number;
};

export type CompileOptions = {
  /** 0 editorial (default), 1 cinematic open, 2 direct/plain */
  voice?: 0 | 1 | 2;
};
