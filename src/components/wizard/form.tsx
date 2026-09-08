import {
  ACCESSORIES,
  AGES,
  ARMS,
  BODY_TYPES,
  BOTTOMS,
  BUILTIN_PRESETS,
  COLORS,
  DRESSES,
  EYE_COLORS,
  EXPRESSIONS,
  FRAMINGS,
  GAZES,
  GENDERS,
  HAIR_COLORS,
  HAIR_LENGTHS,
  HAIR_TEXTURES,
  HEIGHTS,
  LENSES,
  LIGHT_QUALITY,
  LIGHT_SOURCES,
  LOCATIONS,
  MEDIUMS,
  MOODS,
  NSFW_ATTIRE,
  OUTERWEAR,
  PHOTO_STYLES,
  POSITIONS,
  SHOES,
  SKIN_TONES,
  SLOT_TYPES,
  TIMES,
  TOPS,
  emptyCharacter,
  emptyGarment,
  type Character,
  type GarmentSlot,
  type Scene,
} from '../../lib/wizard';
import { Icon } from '../Icon';
import { ChipMulti, ChipSelect, ChoiceGrid, CustomRow, More, SelectField, cx } from './fields';

const SLOTS: { slot: GarmentSlot; label: string }[] = [
  { slot: 'top', label: 'Top' },
  { slot: 'bottom', label: 'Bottom' },
  { slot: 'dress', label: 'Dress' },
  { slot: 'outerwear', label: 'Jacket' },
  { slot: 'shoes', label: 'Shoes' },
  { slot: 'accessory', label: 'Accessory' },
];

const FIRST: Record<GarmentSlot, string> = {
  top: TOPS[0]!.id,
  bottom: BOTTOMS[0]!.id,
  dress: DRESSES[0]!.id,
  outerwear: OUTERWEAR[0]!.id,
  shoes: SHOES[0]!.id,
  accessory: ACCESSORIES[0]!.id,
  custom: '',
};

function patchChar(onScene: (fn: (s: Scene) => Scene) => void, fn: (c: Character) => Character) {
  onScene((s) => ({
    ...s,
    characters: s.characters.length
      ? s.characters.map((c, i) => (i === 0 ? fn(c) : c))
      : [fn(emptyCharacter())],
  }));
}

function NoPerson({ onAdd, copy }: { onAdd: () => void; copy: string }) {
  return (
    <div className="wiz-stack">
      <div>
        <h2 className="wiz-h">No person in this look</h2>
        <p className="wiz-lead">{copy}</p>
      </div>
      <button type="button" className="wiz-btn" onClick={onAdd}>
        <Icon name="plus" size={16} />
        Add a person
      </button>
    </div>
  );
}

export function StartStep({
  scene,
  onScene,
  onPick,
}: {
  scene: Scene;
  onScene: (fn: (s: Scene) => Scene) => void;
  onPick: (id: string) => void;
}) {
  const presets = BUILTIN_PRESETS.filter((p) => scene.nsfw || !p.nsfw);
  return (
    <div className="wiz-stack">
      <div>
        <h2 className="wiz-h">Start from a look</h2>
        <p className="wiz-lead">
          Pick a preset. You can change details on the next screens, or skip ahead.
        </p>
      </div>
      <div className="wiz-presets">
        {presets.map((p) => (
          <button key={p.id} type="button" className="wiz-preset" onClick={() => onPick(p.id)}>
            <span className="wiz-preset-name">{p.name}</span>
            <span className="wiz-preset-hint">{p.hint}</span>
          </button>
        ))}
      </div>
      <div className="wiz-adult">
        <button
          type="button"
          onClick={() =>
            onScene((s) => ({
              ...s,
              nsfw: !s.nsfw,
              moods: !s.nsfw ? s.moods : s.moods.filter((m) => m !== 'sensual' && m !== 'erotic'),
              characters: s.characters.map((c) => ({
                ...c,
                nsfwAttire: !s.nsfw ? c.nsfwAttire : undefined,
              })),
            }))
          }
          className={cx('wiz-adult-toggle', scene.nsfw && 'is-on')}
        >
          <span>Adult content (18+)</span>
          <span>{scene.nsfw ? 'On' : 'Off'}</span>
        </button>
        {scene.nsfw && (
          <p className="wiz-note">
            Characters are always 18 or older. Local generation may still refuse explicit prompts.
          </p>
        )}
      </div>
    </div>
  );
}

export function PersonStep({
  scene,
  onScene,
}: {
  scene: Scene;
  onScene: (fn: (s: Scene) => Scene) => void;
}) {
  const ch = scene.characters[0];
  const nsfw = scene.nsfw;
  if (!ch) {
    return (
      <NoPerson
        copy="This preset has no one in the frame. Add a person, or continue to Place."
        onAdd={() => onScene((s) => ({ ...s, characters: [emptyCharacter()] }))}
      />
    );
  }
  return (
    <div className="wiz-stack">
      <div>
        <h2 className="wiz-h">Who’s in the frame?</h2>
        <p className="wiz-lead">A few basics. Skip anything you don’t care about.</p>
      </div>
      <ChoiceGrid
        label="Gender"
        options={GENDERS}
        value={ch.gender}
        nsfw={nsfw}
        onChange={(gender) => patchChar(onScene, (c) => ({ ...c, gender }))}
      />
      <div className="wiz-grid-2">
        <SelectField
          label="Age"
          options={AGES}
          value={ch.age}
          nsfw={nsfw}
          onChange={(age) => patchChar(onScene, (c) => ({ ...c, age, ageRange: undefined }))}
        />
        <SelectField
          label="Expression"
          options={EXPRESSIONS}
          value={ch.expression}
          nsfw={nsfw}
          onChange={(expression) => patchChar(onScene, (c) => ({ ...c, expression }))}
        />
      </div>
      <More>
        <SelectField
          label="Eyes"
          options={EYE_COLORS}
          value={ch.eyeColor}
          nsfw={nsfw}
          onChange={(eyeColor) => patchChar(onScene, (c) => ({ ...c, eyeColor }))}
        />
        <SelectField
          label="Build"
          options={BODY_TYPES}
          value={ch.bodyType}
          nsfw={nsfw}
          onChange={(bodyType) => patchChar(onScene, (c) => ({ ...c, bodyType }))}
        />
        <SelectField
          label="Skin"
          options={SKIN_TONES}
          value={ch.skinTone}
          nsfw={nsfw}
          onChange={(skinTone) => patchChar(onScene, (c) => ({ ...c, skinTone }))}
        />
        <SelectField
          label="Height"
          options={HEIGHTS}
          value={ch.height}
          nsfw={nsfw}
          onChange={(height) => patchChar(onScene, (c) => ({ ...c, height }))}
        />
      </More>
    </div>
  );
}

export function LookStep({
  scene,
  onScene,
}: {
  scene: Scene;
  onScene: (fn: (s: Scene) => Scene) => void;
}) {
  const ch = scene.characters[0];
  const nsfw = scene.nsfw;
  if (!ch) {
    return (
      <NoPerson
        copy="Add a person to choose hair, clothes, and pose."
        onAdd={() => onScene((s) => ({ ...s, characters: [emptyCharacter()] }))}
      />
    );
  }
  return (
    <div className="wiz-stack">
      <div>
        <h2 className="wiz-h">Hair, clothes, pose</h2>
        <p className="wiz-lead">Color and type are enough. The rest is optional.</p>
      </div>
      <div className="wiz-grid-2">
        <SelectField
          label="Hair color"
          options={HAIR_COLORS}
          value={ch.hairColor}
          nsfw={nsfw}
          onChange={(hairColor) => patchChar(onScene, (c) => ({ ...c, hairColor }))}
        />
        <SelectField
          label="Length"
          options={HAIR_LENGTHS}
          value={ch.hairLength}
          nsfw={nsfw}
          onChange={(hairLength) => patchChar(onScene, (c) => ({ ...c, hairLength }))}
        />
      </div>
      <ChipSelect
        label="Texture"
        options={HAIR_TEXTURES}
        value={ch.hairTexture}
        nsfw={nsfw}
        onChange={(hairTexture) => patchChar(onScene, (c) => ({ ...c, hairTexture }))}
      />
      {nsfw && (
        <ChipSelect
          label="Attire"
          options={NSFW_ATTIRE}
          value={ch.nsfwAttire}
          nsfw={nsfw}
          onChange={(nsfwAttire) => patchChar(onScene, (c) => ({ ...c, nsfwAttire }))}
        />
      )}
      <div className="wiz-clothes">
        <span className="wiz-label">Clothing</span>
        {ch.garments.map((g) => {
          const types = SLOT_TYPES[g.slot] ?? [];
          return (
            <div key={g.id} className="wiz-garment">
              <span className="wiz-slot">{g.slot}</span>
              <select
                value={g.typeId}
                onChange={(e) =>
                  patchChar(onScene, (c) => ({
                    ...c,
                    garments: c.garments.map((x) =>
                      x.id === g.id ? { ...x, typeId: e.target.value } : x,
                    ),
                  }))
                }
                className="wiz-select wiz-select-grow"
              >
                {types.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.label}
                  </option>
                ))}
              </select>
              <select
                value={g.color ?? ''}
                onChange={(e) =>
                  patchChar(onScene, (c) => ({
                    ...c,
                    garments: c.garments.map((x) =>
                      x.id === g.id ? { ...x, color: e.target.value || undefined } : x,
                    ),
                  }))
                }
                className="wiz-select wiz-select-color"
              >
                <option value="">Color</option>
                {COLORS.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.label}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="wiz-icon-btn wiz-icon-btn-danger"
                onClick={() =>
                  patchChar(onScene, (c) => ({
                    ...c,
                    garments: c.garments.filter((x) => x.id !== g.id),
                  }))
                }
                aria-label="Remove"
              >
                <Icon name="trash" size={14} />
              </button>
            </div>
          );
        })}
        <select
          value=""
          onChange={(e) => {
            const slot = e.target.value as GarmentSlot;
            if (!slot) return;
            patchChar(onScene, (c) => ({
              ...c,
              garments: [...c.garments, emptyGarment(slot, FIRST[slot])],
            }));
          }}
          className="wiz-select wiz-select-dashed"
        >
          <option value="">Add clothing…</option>
          {SLOTS.map((s) => (
            <option key={s.slot} value={s.slot}>
              {s.label}
            </option>
          ))}
        </select>
      </div>
      <SelectField
        label="Pose"
        options={POSITIONS}
        value={ch.position}
        nsfw={nsfw}
        onChange={(position) => patchChar(onScene, (c) => ({ ...c, position }))}
      />
      <More>
        <SelectField
          label="Gaze"
          options={GAZES}
          value={ch.gaze}
          nsfw={nsfw}
          onChange={(gaze) => patchChar(onScene, (c) => ({ ...c, gaze }))}
        />
        <SelectField
          label="Arms"
          options={ARMS}
          value={ch.arms}
          nsfw={nsfw}
          onChange={(arms) => patchChar(onScene, (c) => ({ ...c, arms }))}
        />
        <CustomRow
          value={ch.outfitCustom}
          placeholder="Custom clothes note"
          onChange={(outfitCustom) => patchChar(onScene, (c) => ({ ...c, outfitCustom }))}
        />
      </More>
    </div>
  );
}

export function PlaceStep({
  scene,
  onScene,
}: {
  scene: Scene;
  onScene: (fn: (s: Scene) => Scene) => void;
}) {
  const nsfw = scene.nsfw;
  return (
    <div className="wiz-stack">
      <div>
        <h2 className="wiz-h">Place and camera</h2>
        <p className="wiz-lead">Where they are, and how it’s shot.</p>
      </div>
      <SelectField
        label="Location"
        options={LOCATIONS}
        value={scene.location}
        nsfw={nsfw}
        onChange={(location) => onScene((s) => ({ ...s, location }))}
      />
      <div className="wiz-grid-2">
        <SelectField
          label="Time of day"
          options={TIMES}
          value={scene.time}
          nsfw={nsfw}
          onChange={(time) => onScene((s) => ({ ...s, time }))}
        />
        <SelectField
          label="Framing"
          options={FRAMINGS}
          value={scene.framing}
          nsfw={nsfw}
          onChange={(framing) => onScene((s) => ({ ...s, framing }))}
        />
      </div>
      <More>
        <SelectField
          label="Light"
          options={LIGHT_SOURCES}
          value={scene.lightSource}
          nsfw={nsfw}
          onChange={(lightSource) => onScene((s) => ({ ...s, lightSource }))}
        />
        <SelectField
          label="Light quality"
          options={LIGHT_QUALITY}
          value={scene.lightQuality}
          nsfw={nsfw}
          onChange={(lightQuality) => onScene((s) => ({ ...s, lightQuality }))}
        />
        <SelectField
          label="Lens"
          options={LENSES}
          value={scene.lens}
          nsfw={nsfw}
          onChange={(lens) => onScene((s) => ({ ...s, lens }))}
        />
        <SelectField
          label="Medium"
          options={MEDIUMS}
          value={scene.medium}
          nsfw={nsfw}
          onChange={(medium) => onScene((s) => ({ ...s, medium }))}
        />
        <SelectField
          label="Photo style"
          options={PHOTO_STYLES}
          value={scene.photoStyle}
          nsfw={nsfw}
          onChange={(photoStyle) => onScene((s) => ({ ...s, photoStyle }))}
        />
        <ChipMulti
          label="Mood"
          options={MOODS}
          values={scene.moods}
          nsfw={nsfw}
          max={2}
          onChange={(moods) => onScene((s) => ({ ...s, moods }))}
        />
        <CustomRow
          value={scene.additional}
          placeholder="Anything else to include"
          onChange={(additional) => onScene((s) => ({ ...s, additional }))}
        />
      </More>
    </div>
  );
}

export function WizardStepBody({
  step,
  scene,
  onScene,
  onPickPreset,
}: {
  step: number;
  scene: Scene;
  onScene: (fn: (s: Scene) => Scene) => void;
  onPickPreset: (id: string) => void;
}) {
  if (step === 0) return <StartStep scene={scene} onScene={onScene} onPick={onPickPreset} />;
  if (step === 1) return <PersonStep scene={scene} onScene={onScene} />;
  if (step === 2) return <LookStep scene={scene} onScene={onScene} />;
  return <PlaceStep scene={scene} onScene={onScene} />;
}
