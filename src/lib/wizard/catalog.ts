import type { GarmentSlot, Opt } from './types.ts';

function o(id: string, label: string, prompt?: string, extra?: Pick<Opt, 'nsfw' | 'tags'>): Opt {
  return { id, label, prompt: prompt ?? label.toLowerCase(), ...extra };
}

export const AGES: Opt[] = [
  o('18', '18', '18-year-old'),
  o('21', '21', '21-year-old'),
  o('23', '23', '23-year-old'),
  o('25', '25', '25-year-old'),
  o('28', '28', '28-year-old'),
  o('30', '30', '30-year-old'),
  o('32', '32', '32-year-old'),
  o('35', '35', '35-year-old'),
  o('40', '40', '40-year-old'),
  o('45', '45', '45-year-old'),
  o('50', '50', '50-year-old'),
  o('55', '55', '55-year-old'),
  o('60', '60', '60-year-old'),
  o('70', '70', '70-year-old'),
];

export const AGE_RANGES: Opt[] = [
  o('early_20s', 'Early 20s', 'in her early twenties'),
  o('mid_20s', 'Mid 20s', 'in her mid-twenties'),
  o('late_20s', 'Late 20s', 'in her late twenties'),
  o('30s', '30s', 'in her thirties'),
  o('40s', '40s', 'in her forties'),
  o('50s', '50s', 'in her fifties'),
  o('60s', '60s', 'in her sixties'),
];

export const GENDERS: Opt[] = [
  o('woman', 'Woman', 'woman'),
  o('man', 'Man', 'man'),
  o('androgynous', 'Androgynous', 'androgynous person'),
  o('nonbinary', 'Non-binary', 'non-binary person'),
];

export const FACE_SHAPES: Opt[] = [
  o('oval', 'Oval', 'oval face'),
  o('round', 'Round', 'round face'),
  o('heart', 'Heart', 'heart-shaped face'),
  o('square', 'Square', 'square face'),
  o('diamond', 'Diamond', 'diamond-shaped face'),
  o('oblong', 'Oblong', 'oblong face'),
];

export const JAWS: Opt[] = [
  o('soft', 'Soft', 'soft jawline'),
  o('defined', 'Defined', 'defined jawline'),
  o('sharp', 'Sharp', 'sharp jawline'),
  o('rounded', 'Rounded', 'rounded jaw'),
];

export const NOSES: Opt[] = [
  o('straight', 'Straight', 'straight nose'),
  o('button', 'Button', 'small button nose'),
  o('roman', 'Roman', 'roman nose'),
  o('upturned', 'Upturned', 'slightly upturned nose'),
  o('wide', 'Wide', 'wide nose'),
  o('narrow', 'Narrow', 'narrow nose'),
];

export const LIPS: Opt[] = [
  o('full', 'Full', 'full lips'),
  o('thin', 'Thin', 'thin lips'),
  o('heart', 'Heart-shaped', 'heart-shaped lips'),
  o('wide', 'Wide', 'wide mouth'),
  o('cupid', "Cupid's bow", "defined cupid's bow"),
];

export const EYE_COLORS: Opt[] = [
  o('brown', 'Brown', 'brown eyes'),
  o('dark_brown', 'Dark brown', 'dark brown eyes'),
  o('hazel', 'Hazel', 'hazel eyes'),
  o('green', 'Green', 'green eyes'),
  o('blue', 'Blue', 'blue eyes'),
  o('gray', 'Gray', 'gray eyes'),
  o('amber', 'Amber', 'amber eyes'),
  o('dark', 'Near-black', 'very dark eyes'),
];

export const EYE_SHAPES: Opt[] = [
  o('almond', 'Almond', 'almond-shaped eyes'),
  o('round', 'Round', 'round eyes'),
  o('hooded', 'Hooded', 'hooded eyes'),
  o('upturned', 'Upturned', 'upturned eyes'),
  o('deep', 'Deep-set', 'deep-set eyes'),
  o('monolid', 'Monolid', 'monolid eyes'),
];

export const BROWS: Opt[] = [
  o('straight', 'Straight', 'straight eyebrows'),
  o('arched', 'Arched', 'arched eyebrows'),
  o('thick', 'Thick', 'thick eyebrows'),
  o('soft', 'Soft', 'soft eyebrows'),
  o('feathered', 'Feathered', 'feathered eyebrows'),
];

export const FACIAL_FEATURES: Opt[] = [
  o('high_cheeks', 'High cheekbones', 'high cheekbones'),
  o('dimples', 'Dimples', 'dimples'),
  o('cleft', 'Cleft chin', 'cleft chin'),
  o('laugh_lines', 'Laugh lines', 'faint laugh lines'),
  o('undereye', 'Soft undereyes', 'soft undereye shadows'),
];

export const SKIN_TONES: Opt[] = [
  o('fair', 'Fair', 'fair skin'),
  o('light', 'Light', 'light skin'),
  o('light_medium', 'Light-medium', 'light-medium skin'),
  o('olive', 'Olive', 'olive skin'),
  o('tan', 'Tan', 'tan skin'),
  o('medium', 'Medium', 'medium skin'),
  o('brown', 'Brown', 'brown skin'),
  o('deep', 'Deep', 'deep brown skin'),
  o('dark', 'Dark', 'dark skin'),
];

export const SKIN_TEXTURES: Opt[] = [
  o('natural', 'Natural', 'natural skin texture'),
  o('smooth', 'Smooth', 'smooth skin'),
  o('matte', 'Matte', 'matte skin'),
  o('dewy', 'Dewy', 'dewy skin'),
  o('pores', 'Visible pores', 'visible pores and natural texture'),
  o('freckled_tex', 'Freckled', 'freckled skin'),
];

export const SKIN_MARK_KINDS: Opt[] = [
  o('freckles', 'Freckles', 'freckles'),
  o('moles', 'Moles', 'a few moles'),
  o('beauty_marks', 'Beauty marks', 'a beauty mark'),
  o('birthmarks', 'Birthmarks', 'a birthmark'),
  o('scars', 'Scars', 'a faint scar'),
  o('tattoos', 'Tattoos', 'tattoos'),
  o('piercings', 'Piercings', 'piercings'),
];

export const MARK_LOCATIONS: Opt[] = [
  o('face', 'Face', 'on the face'),
  o('cheek', 'Cheek', 'on the cheek'),
  o('collarbone', 'Collarbone', 'along the collarbone'),
  o('shoulder', 'Shoulder', 'on the shoulder'),
  o('arm', 'Arm', 'on the arm'),
  o('hand', 'Hand', 'on the hand'),
  o('neck', 'Neck', 'on the neck'),
  o('back', 'Back', 'across the back'),
  o('leg', 'Leg', 'on the leg'),
  o('chest', 'Chest', 'on the chest', { nsfw: true }),
  o('hip', 'Hip', 'on the hip', { nsfw: true }),
];

export const HEIGHTS: Opt[] = [
  o('petite_h', 'Petite', 'petite stature'),
  o('short', 'Short', 'short'),
  o('average_h', 'Average', 'average height'),
  o('tall', 'Tall', 'tall'),
  o('very_tall', 'Very tall', 'very tall'),
];

export const BODY_TYPES: Opt[] = [
  o('petite', 'Petite', 'petite'),
  o('slim', 'Slim', 'slim'),
  o('lean', 'Lean', 'lean'),
  o('average', 'Average', 'average'),
  o('athletic', 'Athletic', 'athletic'),
  o('muscular', 'Muscular', 'muscular'),
  o('curvy', 'Curvy', 'curvy'),
  o('plus', 'Plus-size', 'plus-size'),
];

export const BUILDS: Opt[] = [
  o('narrow', 'Narrow', 'narrow frame'),
  o('soft', 'Soft', 'soft build'),
  o('compact', 'Compact', 'compact build'),
  o('broad', 'Broad', 'broad build'),
];

export const MUSCLE: Opt[] = [
  o('none', 'None visible', 'soft untoned physique'),
  o('light', 'Light tone', 'light muscle tone'),
  o('defined', 'Defined', 'defined muscles'),
  o('ripped', 'Very defined', 'highly defined musculature'),
];

export const SHOULDERS: Opt[] = [
  o('narrow_s', 'Narrow', 'narrow shoulders'),
  o('average_s', 'Average', 'balanced shoulders'),
  o('broad_s', 'Broad', 'broad shoulders'),
];

export const WAISTS: Opt[] = [
  o('narrow_w', 'Narrow', 'narrow waist'),
  o('average_w', 'Average', 'natural waist'),
  o('wide_w', 'Wide', 'wide waist'),
];

export const HIPS: Opt[] = [
  o('narrow_h', 'Narrow', 'narrow hips'),
  o('average_hp', 'Average', 'balanced hips'),
  o('wide_h', 'Wide', 'wide hips'),
];

export const LEGS: Opt[] = [
  o('short_l', 'Shorter', 'shorter legs'),
  o('balanced_l', 'Balanced', 'balanced leg proportions'),
  o('long_l', 'Long', 'long legs'),
];

export const HAIR_COLORS: Opt[] = [
  o('black', 'Black', 'black'),
  o('dark_brown', 'Dark brown', 'dark brown'),
  o('brown', 'Brown', 'brown'),
  o('light_brown', 'Light brown', 'light brown'),
  o('auburn', 'Auburn', 'auburn'),
  o('red', 'Red', 'red'),
  o('ginger', 'Ginger', 'ginger'),
  o('blonde', 'Blonde', 'blonde'),
  o('dirty_blonde', 'Dirty blonde', 'dirty blonde'),
  o('platinum', 'Platinum', 'platinum blonde'),
  o('white', 'White', 'white'),
  o('gray', 'Gray', 'gray'),
  o('silver', 'Silver', 'silver'),
];

export const HAIR_LENGTHS: Opt[] = [
  o('buzz', 'Buzz', 'buzz-cut'),
  o('pixie', 'Pixie', 'pixie-length'),
  o('short', 'Short', 'short'),
  o('ear', 'Ear-length', 'ear-length'),
  o('chin', 'Chin-length', 'chin-length'),
  o('shoulder', 'Shoulder-length', 'shoulder-length'),
  o('medium', 'Medium', 'medium-length'),
  o('long', 'Long', 'long'),
  o('very_long', 'Very long', 'very long'),
];

export const HAIR_TEXTURES: Opt[] = [
  o('straight', 'Straight', 'straight'),
  o('wavy', 'Wavy', 'wavy'),
  o('curly', 'Curly', 'curly'),
  o('coily', 'Coily', 'coily'),
];

export const CURLS: Opt[] = [
  o('loose', 'Loose waves', 'loose waves'),
  o('defined_curl', 'Defined curls', 'defined curls'),
  o('tight', 'Tight coils', 'tight coils'),
];

export const HAIR_CUTS: Opt[] = [
  o('blunt', 'Blunt', 'blunt cut'),
  o('layered', 'Layered', 'layered cut'),
  o('shag', 'Shag', 'shag cut'),
  o('bob', 'Bob', 'bob'),
  o('lob', 'Lob', 'long bob'),
  o('undercut', 'Undercut', 'undercut'),
];

export const BANGS: Opt[] = [
  o('none', 'None', ''),
  o('full', 'Full bangs', 'full bangs'),
  o('side', 'Side-swept', 'side-swept bangs'),
  o('curtain', 'Curtain', 'curtain bangs'),
  o('wispy', 'Wispy', 'wispy bangs'),
  o('baby', 'Baby bangs', 'baby bangs'),
];

export const PARTINGS: Opt[] = [
  o('middle', 'Middle', 'middle part'),
  o('side', 'Side', 'side part'),
  o('none_p', 'No part', 'no visible part'),
];

export const VOLUME: Opt[] = [
  o('flat', 'Flat', 'flat'),
  o('natural_v', 'Natural', 'natural volume'),
  o('voluminous', 'Voluminous', 'voluminous'),
];

export const HAIRSTYLES: Opt[] = [
  o('down', 'Down', 'worn down'),
  o('ponytail', 'Ponytail', 'in a ponytail'),
  o('bun', 'Bun', 'in a bun'),
  o('messy_bun', 'Messy bun', 'in a messy bun'),
  o('braid', 'Braid', 'in a braid'),
  o('half_up', 'Half-up', 'half-up'),
  o('slicked', 'Slicked back', 'slicked back'),
  o('updo', 'Updo', 'in an updo'),
];

export const HIGHLIGHTS: Opt[] = [
  o('none_hl', 'None', ''),
  o('subtle', 'Subtle', 'subtle highlights'),
  o('balayage', 'Balayage', 'balayage'),
  o('money_piece', 'Money piece', 'face-framing highlights'),
];

export const HAIR_WET: Opt[] = [
  o('dry', 'Dry', 'dry'),
  o('damp', 'Damp', 'slightly damp'),
  o('wet', 'Wet', 'wet, clinging to the skin'),
];

export const COLORS: Opt[] = [
  o('black', 'Black', 'black'),
  o('white', 'White', 'white'),
  o('ivory', 'Ivory', 'ivory'),
  o('cream', 'Cream', 'cream'),
  o('gray', 'Gray', 'gray'),
  o('charcoal', 'Charcoal', 'charcoal'),
  o('navy', 'Navy', 'navy'),
  o('dark_blue', 'Dark blue', 'dark blue'),
  o('blue', 'Blue', 'blue'),
  o('light_blue', 'Light blue', 'light blue'),
  o('teal', 'Teal', 'teal'),
  o('green', 'Green', 'green'),
  o('olive', 'Olive', 'olive'),
  o('brown', 'Brown', 'brown'),
  o('tan', 'Tan', 'tan'),
  o('beige', 'Beige', 'beige'),
  o('camel', 'Camel', 'camel'),
  o('red', 'Red', 'red'),
  o('burgundy', 'Burgundy', 'burgundy'),
  o('pink', 'Pink', 'pink'),
  o('blush', 'Blush', 'blush'),
  o('purple', 'Purple', 'purple'),
  o('yellow', 'Yellow', 'yellow'),
  o('gold', 'Gold', 'gold'),
  o('silver', 'Silver', 'silver'),
  o('nude', 'Nude', 'nude'),
];

export const MATERIALS: Opt[] = [
  o('cotton', 'Cotton', 'cotton'),
  o('linen', 'Linen', 'linen'),
  o('silk', 'Silk', 'silk'),
  o('satin', 'Satin', 'satin'),
  o('wool', 'Wool', 'wool'),
  o('cashmere', 'Cashmere', 'cashmere'),
  o('denim', 'Denim', 'denim'),
  o('leather', 'Leather', 'leather'),
  o('suede', 'Suede', 'suede'),
  o('knit', 'Knit', 'knit'),
  o('chiffon', 'Chiffon', 'chiffon'),
  o('lace', 'Lace', 'lace', { nsfw: true }),
  o('mesh', 'Mesh', 'sheer mesh', { nsfw: true }),
  o('velvet', 'Velvet', 'velvet'),
];

export const PATTERNS: Opt[] = [
  o('solid', 'Solid', ''),
  o('stripes', 'Stripes', 'striped'),
  o('plaid', 'Plaid', 'plaid'),
  o('floral', 'Floral', 'floral'),
  o('dots', 'Dots', 'polka-dot'),
  o('abstract', 'Abstract', 'abstract-print'),
];

export const FITS: Opt[] = [
  o('fitted', 'Fitted', 'fitted'),
  o('tailored', 'Tailored', 'tailored'),
  o('relaxed', 'Relaxed', 'relaxed'),
  o('oversized', 'Oversized', 'oversized'),
  o('cropped', 'Cropped', 'cropped'),
  o('bodycon', 'Bodycon', 'bodycon', { nsfw: true }),
];

export const NECKLINES: Opt[] = [
  o('crew', 'Crew', 'crew neck'),
  o('vneck', 'V-neck', 'V-neck'),
  o('scoop', 'Scoop', 'scoop neck'),
  o('collar', 'Collar', 'collared'),
  o('off_shoulder', 'Off-shoulder', 'off-shoulder'),
  o('halter', 'Halter', 'halter neck'),
  o('plunge', 'Plunge', 'plunging neckline', { nsfw: true }),
];

export const SLEEVES: Opt[] = [
  o('sleeveless', 'Sleeveless', 'sleeveless'),
  o('short', 'Short', 'short-sleeved'),
  o('three_quarter', '3/4', 'three-quarter sleeves'),
  o('long', 'Long', 'long-sleeved'),
];

export const STATES: Opt[] = [
  o('clean', 'Clean', ''),
  o('wrinkled', 'Wrinkled', 'slightly wrinkled'),
  o('tucked', 'Tucked', 'tucked in'),
  o('untucked', 'Untucked', 'untucked'),
  o('rolled', 'Sleeves rolled', 'sleeves rolled up'),
  o('buttoned', 'Buttoned', 'buttoned'),
  o('open', 'Open', 'worn open'),
  o('wet', 'Wet', 'wet'),
  o('unbuttoned', 'Unbuttoned', 'partially unbuttoned', { nsfw: true }),
];

export const TOPS: Opt[] = [
  o('tshirt', 'T-shirt', 'T-shirt', { tags: ['casual'] }),
  o('shirt', 'Shirt', 'button-up shirt', { tags: ['smart', 'office'] }),
  o('blouse', 'Blouse', 'blouse', { tags: ['smart'] }),
  o('sweater', 'Sweater', 'sweater', { tags: ['cold'] }),
  o('tank', 'Tank top', 'tank top', { tags: ['warm', 'casual'] }),
  o('crop', 'Crop top', 'crop top', { tags: ['casual', 'warm'] }),
  o('bodysuit', 'Bodysuit', 'bodysuit'),
  o('hoodie', 'Hoodie', 'hoodie', { tags: ['casual'] }),
  o('camisole', 'Camisole', 'camisole', { tags: ['soft'] }),
  o('knit_top', 'Knit top', 'knit top'),
];

export const BOTTOMS: Opt[] = [
  o('jeans', 'Jeans', 'jeans', { tags: ['casual'] }),
  o('trousers', 'Trousers', 'trousers', { tags: ['smart', 'office'] }),
  o('chinos', 'Chinos', 'chinos', { tags: ['smart'] }),
  o('shorts', 'Shorts', 'shorts', { tags: ['warm', 'casual'] }),
  o('skirt', 'Skirt', 'skirt'),
  o('mini_skirt', 'Mini skirt', 'mini skirt'),
  o('leggings', 'Leggings', 'leggings', { tags: ['casual'] }),
  o('sweatpants', 'Sweatpants', 'sweatpants', { tags: ['casual'] }),
];

export const DRESSES: Opt[] = [
  o('midi', 'Midi dress', 'midi dress'),
  o('maxi', 'Maxi dress', 'maxi dress'),
  o('mini', 'Mini dress', 'mini dress'),
  o('slip', 'Slip dress', 'slip dress'),
  o('wrap', 'Wrap dress', 'wrap dress'),
  o('evening', 'Evening dress', 'evening dress', { tags: ['formal'] }),
  o('sundress', 'Sundress', 'sundress', { tags: ['warm', 'beach'] }),
];

export const OUTERWEAR: Opt[] = [
  o('jacket', 'Jacket', 'jacket'),
  o('coat', 'Coat', 'coat', { tags: ['cold'] }),
  o('blazer', 'Blazer', 'blazer', { tags: ['office', 'smart'] }),
  o('cardigan', 'Cardigan', 'cardigan'),
  o('leather_j', 'Leather jacket', 'leather jacket', { tags: ['casual'] }),
  o('trench', 'Trench', 'trench coat'),
];

export const SHOES: Opt[] = [
  o('sneakers', 'Sneakers', 'sneakers', { tags: ['casual'] }),
  o('boots', 'Boots', 'boots'),
  o('heels', 'Heels', 'heels'),
  o('sandals', 'Sandals', 'sandals', { tags: ['warm', 'beach'] }),
  o('flats', 'Flats', 'flats'),
  o('loafers', 'Loafers', 'loafers', { tags: ['smart'] }),
  o('barefoot', 'Barefoot', 'barefoot'),
];

export const ACCESSORIES: Opt[] = [
  o('necklace', 'Necklace', 'necklace'),
  o('earrings', 'Earrings', 'earrings'),
  o('bracelet', 'Bracelet', 'bracelet'),
  o('watch', 'Watch', 'watch'),
  o('glasses', 'Glasses', 'glasses'),
  o('sunglasses', 'Sunglasses', 'sunglasses'),
  o('hat', 'Hat', 'hat'),
  o('bag', 'Bag', 'bag'),
  o('scarf', 'Scarf', 'scarf', { tags: ['cold'] }),
  o('gloves', 'Gloves', 'gloves'),
  o('hair_acc', 'Hair accessory', 'hair accessory'),
  o('jewelry', 'Jewelry', 'jewelry'),
  o('choker', 'Choker', 'choker', { nsfw: true }),
];

export const NSFW_ATTIRE: Opt[] = [
  o('lingerie', 'Lingerie', 'lingerie', { nsfw: true, tags: ['intimate'] }),
  o('lace_lingerie', 'Lace lingerie', 'black lace lingerie', { nsfw: true, tags: ['intimate'] }),
  o('silk_slip', 'Silk slip', 'silk slip', { nsfw: true, tags: ['intimate'] }),
  o('bra_panties', 'Bra and panties', 'matching bra and panties', {
    nsfw: true,
    tags: ['intimate'],
  }),
  o('robe_open', 'Open robe', 'an open silk robe', { nsfw: true, tags: ['intimate'] }),
  o('towel', 'Towel', 'a towel wrapped around the body', { nsfw: true, tags: ['intimate'] }),
  o('underwear_only', 'Underwear only', 'underwear only', { nsfw: true, tags: ['intimate'] }),
  o('implied_nude', 'Implied nude', 'implied nude, body turned, strategic shadow', {
    nsfw: true,
    tags: ['intimate'],
  }),
  o('artistic_nude', 'Artistic nude', 'tasteful artistic nude', { nsfw: true, tags: ['intimate'] }),
  o('nude', 'Nude', 'nude', { nsfw: true, tags: ['intimate'] }),
  o('explicit_nude', 'Explicit nude', 'fully nude, anatomy clearly visible', {
    nsfw: true,
    tags: ['intimate'],
  }),
];

export const SLOT_TYPES: Record<GarmentSlot, Opt[]> = {
  top: TOPS,
  bottom: BOTTOMS,
  dress: DRESSES,
  outerwear: OUTERWEAR,
  shoes: SHOES,
  accessory: ACCESSORIES,
  custom: [],
};

export const POSITIONS: Opt[] = [
  o('standing', 'Standing', 'stands', { tags: ['upright'] }),
  o('sitting', 'Sitting', 'sits', { tags: ['seated'] }),
  o('kneeling', 'Kneeling', 'kneels'),
  o('squatting', 'Squatting', 'squats'),
  o('crouching', 'Crouching', 'crouches'),
  o('walking', 'Walking', 'walks', { tags: ['motion'] }),
  o('running', 'Running', 'runs', { tags: ['motion'] }),
  o('lying', 'Lying down', 'lies', { tags: ['reclined'] }),
  o('leaning', 'Leaning', 'leans'),
  o('reclining', 'Reclining', 'reclines', { tags: ['reclined'] }),
  o('on_bed', 'On a bed', 'reclines on a bed', { nsfw: true, tags: ['intimate', 'reclined'] }),
  o('arched', 'Arched back', 'arches her back', { nsfw: true, tags: ['intimate'] }),
];

export const TORSOS: Opt[] = [
  o('facing', 'Facing camera', 'facing the camera'),
  o('three_q', 'Three-quarter', 'turned three-quarter toward the camera'),
  o('left', 'Turned left', 'turned slightly left'),
  o('right', 'Turned right', 'turned slightly right'),
  o('profile', 'Profile', 'in profile'),
  o('lean_fwd', 'Lean forward', 'leaning slightly forward'),
  o('lean_back', 'Lean back', 'leaning back'),
];

export const HEADS: Opt[] = [
  o('straight', 'Straight', 'head level'),
  o('tilt_l', 'Tilted left', 'head tilted left'),
  o('tilt_r', 'Tilted right', 'head tilted right'),
  o('up', 'Looking up', 'chin lifted'),
  o('down', 'Looking down', 'chin lowered'),
  o('turn_l', 'Turned left', 'head turned left'),
  o('turn_r', 'Turned right', 'head turned right'),
  o('over_shoulder', 'Over shoulder', 'looking back over one shoulder'),
];

export const ARMS: Opt[] = [
  o('sides', 'At sides', 'arms at her sides'),
  o('crossed', 'Crossed', 'arms crossed'),
  o('raised', 'Raised', 'arms raised'),
  o('behind', 'Behind back', 'hands behind her back'),
  o('hip', 'One hand on hip', 'one hand on her hip'),
  o('pocket', 'One hand in pocket', 'one hand resting in a pocket'),
  o('pockets', 'Both in pockets', 'both hands in her pockets'),
  o('hair', 'Touching hair', 'one hand in her hair'),
  o('holding', 'Holding object', 'holding an object'),
  o('across_chest', 'Across chest', 'arms loosely across her chest', { nsfw: true }),
];

export const HANDS: Opt[] = [
  o('open', 'Open', 'open hands'),
  o('relaxed', 'Relaxed', 'relaxed hands'),
  o('closed', 'Closed', 'closed hands'),
  o('face', 'Touching face', 'one hand near her face'),
  o('hair_h', 'In hair', 'fingers in her hair'),
  o('pockets_h', 'In pockets', 'hands in pockets'),
  o('holding_h', 'Holding', 'holding something'),
];

export const LEG_POSES: Opt[] = [
  o('together', 'Together', 'legs together'),
  o('apart', 'Slightly apart', 'feet slightly apart'),
  o('crossed', 'Crossed', 'legs crossed'),
  o('forward', 'One forward', 'one leg forward'),
  o('bent', 'One knee bent', 'one knee bent'),
  o('walk_st', 'Walking stance', 'in a walking stance'),
];

export const GAZES: Opt[] = [
  o('camera', 'At camera', 'looking toward the camera'),
  o('away', 'Away', 'looking away'),
  o('left', 'Left', 'looking left'),
  o('right', 'Right', 'looking right'),
  o('down', 'Down', 'looking down'),
  o('other', 'At another person', 'looking at the other person'),
  o('closed', 'Eyes closed', 'eyes closed'),
  o('over_shoulder_g', 'Over the shoulder', 'glancing back over her shoulder'),
];

export const EXPRESSIONS: Opt[] = [
  o('neutral', 'Neutral', 'a calm, neutral expression'),
  o('happy', 'Happy', 'a happy expression'),
  o('smiling', 'Smiling', 'a warm smile'),
  o('slight_smile', 'Slight smile', 'a subtle smile'),
  o('laughing', 'Laughing', 'laughing'),
  o('sad', 'Sad', 'a sad expression'),
  o('angry', 'Angry', 'an angry expression'),
  o('serious', 'Serious', 'a serious expression'),
  o('confident', 'Confident', 'a confident expression'),
  o('shy', 'Shy', 'a shy expression'),
  o('playful', 'Playful', 'a playful expression'),
  o('surprised', 'Surprised', 'a look of surprise'),
  o('worried', 'Worried', 'a worried expression'),
  o('tired', 'Tired', 'a tired expression'),
  o('thoughtful', 'Thoughtful', 'a thoughtful expression'),
  o('flirty', 'Flirty', 'a flirty expression'),
  o('seductive', 'Seductive', 'a seductive look', { nsfw: true }),
  o('bedroom', 'Bedroom eyes', 'heavy-lidded bedroom eyes', { nsfw: true }),
  o('flushed', 'Flushed', 'flushed cheeks and parted lips', { nsfw: true }),
];

export const MOUTHS: Opt[] = [
  o('closed', 'Closed', 'mouth closed'),
  o('slight', 'Slight smile', 'lips curved in a slight smile'),
  o('wide', 'Wide smile', 'a wide smile'),
  o('open', 'Open', 'mouth slightly open'),
  o('laugh', 'Laughing', 'laughing'),
  o('pursed', 'Pursed', 'pursed lips'),
  o('bite', 'Biting lip', 'biting her lower lip', { nsfw: true }),
  o('parted', 'Parted', 'lips parted', { nsfw: true }),
];

export const EYE_EXPR: Opt[] = [
  o('relaxed', 'Relaxed', 'relaxed eyes'),
  o('wide', 'Wide', 'wide eyes'),
  o('narrowed', 'Narrowed', 'narrowed eyes'),
  o('closed_e', 'Closed', 'eyes closed'),
  o('intense', 'Intense', 'an intense gaze'),
  o('soft', 'Soft', 'a soft gaze'),
];

export const LOCATIONS: Opt[] = [
  o('bedroom', 'Bedroom', 'a bedroom', { tags: ['indoor', 'private', 'intimate'] }),
  o('living', 'Living room', 'a living room', { tags: ['indoor'] }),
  o('bathroom', 'Bathroom', 'a bathroom', { tags: ['indoor', 'private', 'intimate'] }),
  o('kitchen', 'Kitchen', 'a kitchen', { tags: ['indoor'] }),
  o('office', 'Office', 'an office', { tags: ['indoor', 'office'] }),
  o('cafe', 'Café', 'a café', { tags: ['indoor', 'public'] }),
  o('restaurant', 'Restaurant', 'a restaurant', { tags: ['indoor', 'public'] }),
  o('street', 'Street', 'a city street', { tags: ['outdoor', 'urban'] }),
  o('city', 'City', 'a city', { tags: ['outdoor', 'urban'] }),
  o('rooftop', 'Rooftop', 'a rooftop', { tags: ['outdoor', 'urban'] }),
  o('forest', 'Forest', 'a forest', { tags: ['outdoor', 'nature'] }),
  o('beach', 'Beach', 'a beach', { tags: ['outdoor', 'warm', 'beach'] }),
  o('mountain', 'Mountain', 'a mountain landscape', { tags: ['outdoor', 'nature', 'cold'] }),
  o('desert', 'Desert', 'a desert', { tags: ['outdoor', 'warm'] }),
  o('studio', 'Studio', 'a photography studio', { tags: ['indoor', 'studio'] }),
  o('apartment', 'Apartment', 'an apartment', { tags: ['indoor', 'private'] }),
  o('hotel', 'Hotel room', 'a hotel room', { tags: ['indoor', 'private', 'intimate'] }),
  o('nightclub', 'Nightclub', 'a nightclub', { tags: ['indoor', 'night'] }),
  o('library', 'Library', 'a library', { tags: ['indoor'] }),
  o('train', 'Train', 'a train carriage', { tags: ['indoor'] }),
  o('park', 'Park', 'a park', { tags: ['outdoor', 'nature'] }),
  o('bar', 'Bar', 'a dim bar', { tags: ['indoor', 'night'] }),
  o('futuristic', 'Futuristic', 'a futuristic interior', { tags: ['indoor'] }),
  o('warehouse', 'Warehouse', 'an industrial warehouse', { tags: ['indoor'] }),
];

export const AMBIANCES: Opt[] = [
  o('clean', 'Clean', 'clean and uncluttered'),
  o('messy', 'Messy', 'slightly messy'),
  o('minimal', 'Minimal', 'minimal'),
  o('luxury', 'Luxury', 'luxurious'),
  o('rustic', 'Rustic', 'rustic'),
  o('abandoned', 'Abandoned', 'abandoned'),
  o('crowded', 'Crowded', 'crowded'),
  o('empty', 'Empty', 'empty'),
  o('cozy', 'Cozy', 'cozy'),
  o('industrial', 'Industrial', 'industrial'),
  o('futuristic_a', 'Futuristic', 'futuristic'),
];

export const TIMES: Opt[] = [
  o('morning', 'Morning', 'morning', { tags: ['day'] }),
  o('afternoon', 'Afternoon', 'afternoon', { tags: ['day'] }),
  o('golden', 'Golden hour', 'golden hour', { tags: ['warm', 'day'] }),
  o('sunset', 'Sunset', 'sunset', { tags: ['warm'] }),
  o('blue', 'Blue hour', 'blue hour', { tags: ['cool'] }),
  o('night', 'Night', 'night', { tags: ['night'] }),
  o('midnight', 'Midnight', 'midnight', { tags: ['night'] }),
];

export const WEATHERS: Opt[] = [
  o('sunny', 'Sunny', 'sunny', { tags: ['day'] }),
  o('cloudy', 'Cloudy', 'overcast'),
  o('rain', 'Rain', 'light rain'),
  o('heavy_rain', 'Heavy rain', 'heavy rain'),
  o('snow', 'Snow', 'snow', { tags: ['cold'] }),
  o('fog', 'Fog', 'fog'),
  o('storm', 'Storm', 'a storm'),
  o('wind', 'Wind', 'wind'),
];

export const FRAMINGS: Opt[] = [
  o('ecu', 'Extreme close-up', 'extreme close-up'),
  o('cu', 'Close-up', 'close-up'),
  o('head', 'Head and shoulders', 'head-and-shoulders portrait'),
  o('half', 'Half body', 'half-body shot'),
  o('tq', 'Three-quarter', 'three-quarter body shot'),
  o('full', 'Full body', 'full-body shot'),
  o('wide', 'Wide', 'wide shot'),
  o('ewide', 'Extreme wide', 'extreme wide shot'),
];

export const ANGLES: Opt[] = [
  o('eye', 'Eye level', 'eye-level'),
  o('low', 'Low angle', 'low angle'),
  o('high', 'High angle', 'high angle'),
  o('overhead', 'Overhead', 'overhead'),
  o('worms', "Worm's-eye", "worm's-eye view"),
  o('dutch', 'Dutch angle', 'dutch angle'),
  o('profile_a', 'Profile', 'profile angle'),
  o('three_q_a', 'Three-quarter', 'three-quarter angle'),
];

export const LENSES: Opt[] = [
  o('24', '24mm', '24mm wide lens'),
  o('28', '28mm', '28mm lens'),
  o('35', '35mm', '35mm lens'),
  o('50', '50mm', '50mm lens'),
  o('85', '85mm', '85mm portrait lens'),
  o('105', '105mm', '105mm lens'),
  o('135', '135mm', '135mm telephoto lens'),
  o('tele', 'Telephoto', 'telephoto lens'),
];

export const DOFS: Opt[] = [
  o('deep', 'Deep', 'deep depth of field'),
  o('moderate', 'Moderate', 'moderate depth of field'),
  o('shallow', 'Shallow', 'shallow depth of field'),
  o('very_shallow', 'Very shallow', 'very shallow depth of field, creamy bokeh'),
];

export const FOCUSES: Opt[] = [
  o('face', 'Face', 'focus on the face'),
  o('eyes', 'Eyes', 'sharp focus on the eyes'),
  o('subject', 'Subject', 'focus on the subject'),
  o('fg', 'Foreground', 'foreground focus'),
  o('bg', 'Background', 'background focus'),
];

export const CAMERA_CHARS: Opt[] = [
  o('handheld', 'Handheld', 'handheld'),
  o('tripod', 'Tripod', 'tripod-stable'),
  o('dynamic', 'Dynamic', 'dynamic camera'),
  o('static', 'Static', 'static frame'),
  o('motion_blur', 'Motion blur', 'slight motion blur'),
  o('long_exp', 'Long exposure', 'long exposure'),
];

export const LIGHT_SOURCES: Opt[] = [
  o('natural', 'Natural', 'natural light', { tags: ['day'] }),
  o('window', 'Window', 'window light', { tags: ['indoor'] }),
  o('studio', 'Studio', 'studio light', { tags: ['studio'] }),
  o('sun', 'Sunlight', 'sunlight', { tags: ['day'] }),
  o('moon', 'Moonlight', 'moonlight', { tags: ['night'] }),
  o('candle', 'Candlelight', 'candlelight', { tags: ['night', 'intimate'] }),
  o('neon', 'Neon', 'neon light', { tags: ['night'] }),
  o('practical', 'Practical', 'practical lamps'),
];

export const LIGHT_DIRS: Opt[] = [
  o('front', 'Front', 'front light'),
  o('side', 'Side', 'side light'),
  o('back', 'Back', 'backlight'),
  o('rim', 'Rim', 'rim light'),
  o('top', 'Top', 'top light'),
  o('bottom', 'Bottom', 'underlighting'),
  o('multi', 'Multiple', 'multiple sources'),
];

export const LIGHT_QUALITY: Opt[] = [
  o('soft', 'Soft', 'soft'),
  o('diffused', 'Diffused', 'diffused'),
  o('hard', 'Hard', 'hard'),
  o('dramatic', 'Dramatic', 'dramatic'),
];

export const LIGHT_TEMP: Opt[] = [
  o('warm', 'Warm', 'warm'),
  o('neutral', 'Neutral', 'neutral'),
  o('cool', 'Cool', 'cool'),
];

export const LIGHT_INTENSITY: Opt[] = [
  o('subtle', 'Subtle', 'subtle'),
  o('moderate', 'Moderate', 'moderate'),
  o('strong', 'Strong', 'strong'),
  o('dramatic_i', 'Dramatic', 'dramatic'),
];

export const LIGHT_EFFECTS: Opt[] = [
  o('rim_e', 'Rim lighting', 'rim lighting'),
  o('flare', 'Lens flare', 'lens flare'),
  o('volumetric', 'Volumetric', 'volumetric light'),
  o('rays', 'Light rays', 'visible light rays'),
  o('reflections', 'Reflections', 'reflections'),
  o('shadows', 'Shadows', 'shaped shadows'),
];

export const COMPOSITIONS: Opt[] = [
  o('centered', 'Centered', 'centered composition'),
  o('thirds', 'Rule of thirds', 'rule of thirds'),
  o('sym', 'Symmetrical', 'symmetrical composition'),
  o('asym', 'Asymmetrical', 'asymmetrical composition'),
  o('negative', 'Negative space', 'generous negative space'),
  o('leading', 'Leading lines', 'leading lines'),
  o('fg_frame', 'Foreground framing', 'foreground framing'),
  o('layered', 'Layered', 'layered composition'),
  o('dynamic_c', 'Dynamic', 'dynamic composition'),
  o('minimal_c', 'Minimal', 'minimal composition'),
];

export const SUBJECT_POS: Opt[] = [
  o('center', 'Center', 'subject centered'),
  o('left', 'Left', 'subject on the left'),
  o('right', 'Right', 'subject on the right'),
  o('fg', 'Foreground', 'subject in the foreground'),
  o('bg', 'Background', 'subject in the background'),
];

export const MEDIUMS: Opt[] = [
  o('photo', 'Photography', 'photograph'),
  o('cinematic', 'Cinematic', 'cinematic still'),
  o('illustration', 'Illustration', 'illustration'),
  o('digital', 'Digital art', 'digital art'),
  o('oil', 'Oil painting', 'oil painting'),
  o('watercolor', 'Watercolor', 'watercolor painting'),
  o('render3d', '3D render', '3D render'),
  o('anime', 'Anime', 'anime illustration'),
  o('comic', 'Comic', 'comic-book art'),
  o('concept', 'Concept art', 'concept art'),
];

export const PHOTO_STYLES: Opt[] = [
  o('portrait', 'Portrait', 'portrait photography'),
  o('editorial', 'Fashion editorial', 'fashion editorial'),
  o('street', 'Street', 'street photography'),
  o('doc', 'Documentary', 'documentary photography'),
  o('film_still', 'Film still', 'cinematic film still'),
  o('studio_p', 'Studio', 'studio photography'),
  o('polaroid', 'Polaroid', 'Polaroid photograph'),
  o('analog', 'Analog', 'analog film photography'),
  o('phone', 'Smartphone', 'smartphone snapshot'),
];

export const ERAS: Opt[] = [
  o('contemporary', 'Contemporary', 'contemporary'),
  o('vintage', 'Vintage', 'vintage'),
  o('1920s', '1920s', '1920s'),
  o('1950s', '1950s', '1950s'),
  o('1970s', '1970s', '1970s'),
  o('1980s', '1980s', '1980s'),
  o('1990s', '1990s', '1990s'),
  o('y2k', 'Y2K', 'Y2K'),
  o('futuristic_e', 'Futuristic', 'futuristic'),
];

export const FINISHES: Opt[] = [
  o('photoreal', 'Photorealistic', 'photorealistic'),
  o('natural_f', 'Natural', 'natural'),
  o('cinematic_f', 'Cinematic', 'cinematic'),
  o('editorial_f', 'Editorial', 'editorial'),
  o('raw', 'Raw', 'raw'),
  o('polished', 'Polished', 'polished'),
  o('dreamlike', 'Dreamlike', 'dreamlike'),
  o('gritty', 'Gritty', 'gritty'),
];

export const COLOR_MOODS: Opt[] = [
  o('warm_c', 'Warm', 'warm colors'),
  o('cool_c', 'Cool', 'cool colors'),
  o('neutral_c', 'Neutral', 'neutral colors'),
  o('vibrant', 'Vibrant', 'vibrant color'),
  o('muted', 'Muted', 'muted colors'),
  o('muted_warm', 'Muted warm', 'muted warm colors'),
  o('pastel', 'Pastel', 'pastel colors'),
  o('mono', 'Monochrome', 'monochrome'),
  o('bw', 'Black and white', 'black and white'),
];

export const SATURATIONS: Opt[] = [
  o('desat', 'Desaturated', 'desaturated'),
  o('natural_s', 'Natural', 'natural saturation'),
  o('rich', 'Rich', 'rich saturation'),
];

export const CONTRASTS: Opt[] = [
  o('low_c', 'Low', 'low contrast'),
  o('medium_c', 'Medium', 'medium contrast'),
  o('high_c', 'High', 'high contrast'),
];

export const MOODS: Opt[] = [
  o('cozy', 'Cozy', 'cozy atmosphere'),
  o('intimate', 'Intimate', 'intimate atmosphere'),
  o('romantic', 'Romantic', 'romantic mood'),
  o('dramatic', 'Dramatic', 'dramatic mood'),
  o('mysterious', 'Mysterious', 'mysterious mood'),
  o('melancholic', 'Melancholic', 'melancholic mood'),
  o('energetic', 'Energetic', 'energetic mood'),
  o('peaceful', 'Peaceful', 'peaceful atmosphere'),
  o('dark', 'Dark', 'dark mood'),
  o('dreamlike_m', 'Dreamlike', 'dreamlike atmosphere'),
  o('luxurious', 'Luxurious', 'luxurious mood'),
  o('tense', 'Tense', 'tense atmosphere'),
  o('playful_m', 'Playful', 'playful mood'),
  o('nostalgic', 'Nostalgic', 'nostalgic mood'),
  o('futuristic_m', 'Futuristic', 'futuristic mood'),
  o('sensual', 'Sensual', 'sensual atmosphere', { nsfw: true }),
  o('erotic', 'Erotic', 'erotic atmosphere', { nsfw: true }),
];

export const RELATIONSHIPS: Opt[] = [
  o('friends', 'Friends', 'friends'),
  o('couple', 'Couple', 'a couple'),
  o('family', 'Family', 'family'),
  o('colleagues', 'Colleagues', 'colleagues'),
  o('strangers', 'Strangers', 'strangers'),
  o('rivals', 'Rivals', 'rivals'),
];

export const INTERACTIONS: Opt[] = [
  o('talking', 'Talking', 'talking'),
  o('looking', 'Looking at each other', 'looking at each other'),
  o('hands', 'Holding hands', 'holding hands'),
  o('hugging', 'Hugging', 'hugging'),
  o('walking_t', 'Walking together', 'walking together'),
  o('sitting_t', 'Sitting together', 'sitting together'),
  o('working', 'Working together', 'working together'),
  o('kissing', 'Kissing', 'kissing', { nsfw: true }),
  o('embrace', 'Intimate embrace', 'in a close intimate embrace', { nsfw: true }),
];

export const POSITIONINGS: Opt[] = [
  o('side', 'Side by side', 'side by side'),
  o('left_right', 'Left / right', 'one on the left, one on the right'),
  o('front_back', 'Front / back', 'one slightly in front of the other'),
  o('close', 'Close', 'standing close together'),
  o('apart', 'Apart', 'with space between them'),
];

export const PROP_SUGGESTIONS: Opt[] = [
  o('phone', 'Phone', 'a phone'),
  o('laptop', 'Laptop', 'a laptop'),
  o('coffee', 'Coffee cup', 'a coffee cup'),
  o('book', 'Book', 'a book'),
  o('camera', 'Camera', 'a camera'),
  o('flowers', 'Flowers', 'flowers'),
  o('wine', 'Wine glass', 'a wine glass'),
  o('cigarette', 'Cigarette', 'a cigarette'),
  o('keys', 'Keys', 'a set of keys'),
  o('bag_p', 'Handbag', 'a handbag'),
];

export const PROP_POSITIONS: Opt[] = [
  o('hand', 'In hand', 'held in one hand'),
  o('table', 'On a table', 'resting on a table'),
  o('lap', 'In lap', 'in the lap'),
  o('beside', 'Beside', 'beside the subject'),
  o('bg_p', 'Background', 'in the background'),
];

export function findOpt(list: Opt[], id?: string | null): Opt | undefined {
  if (!id) return undefined;
  return list.find((o) => o.id === id);
}

export function promptOf(list: Opt[], id?: string | null): string {
  return findOpt(list, id)?.prompt ?? '';
}

export function labelOf(list: Opt[], id?: string | null): string {
  return findOpt(list, id)?.label ?? '';
}

export function visibleOpts(list: Opt[], nsfw: boolean): Opt[] {
  return nsfw ? list : list.filter((x) => !x.nsfw);
}

export function pick<T>(list: T[]): T {
  return list[Math.floor(Math.random() * list.length)]!;
}

export function pickN<T>(list: T[], n: number): T[] {
  const copy = [...list];
  const out: T[] = [];
  while (out.length < n && copy.length) {
    const i = Math.floor(Math.random() * copy.length);
    out.push(copy.splice(i, 1)[0]!);
  }
  return out;
}
