import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { AuraPreview } from "./AuraPreview";
import {
  api,
  imageUrl,
  type AppState,
  type BatteryMode,
  type Decorations,
  type DesktopStatus,
  type Frequency,
  type Placement,
  placementOf,
  type Quality,
  type Settings,
  type Tree,
  type Tuning,
} from "./api";

type Page = "atmosphere" | "decorations" | "wallpaper" | "system";
type Update = (change: Partial<Settings>) => void;

const PAGES: { id: Page; label: string }[] = [
  { id: "atmosphere", label: "Atmosphere" },
  { id: "decorations", label: "Decorations" },
  { id: "wallpaper", label: "Wallpaper" },
  { id: "system", label: "System" },
];

const PLACEMENTS: { id: Placement; label: string; hint: string }[] = [
  { id: "desktop", label: "Desktop", hint: "Effects stay on the desktop and settle along the tops of smaller windows. Nothing covers an open app." },
  { id: "behind_windows", label: "Behind windows", hint: "Effects stay on the wallpaper, under every window." },
  { id: "everywhere", label: "Everywhere", hint: "Effects drift across everything, windows included. Clicks always pass through." },
];

const BATTERY: { id: BatteryMode; label: string; hint: string }[] = [
  { id: "reduce", label: "Reduce", hint: "Half the particles at a lower frame rate when unplugged." },
  { id: "pause", label: "Pause", hint: "Stop all effects when unplugged." },
  { id: "ignore", label: "No change", hint: "Keep the same quality when unplugged." },
];

const FREQUENCIES: { id: Frequency; label: string }[] = [
  { id: "off", label: "Off" },
  { id: "sometimes", label: "Sometimes" },
  { id: "often", label: "Often" },
];

const NEW_TREE: Tree = { x: 0.12, size: 1, baubles: true, lights: true, candy: true, star: true };
const MAX_DECORATIONS = 3;
const heading = "mb-3 text-sm font-semibold text-muted";
const panel = "divide-y divide-line rounded-xl border border-line bg-card px-5";
const quietButton = "rounded-lg border border-line px-3 py-1.5 text-sm font-medium hover:border-muted";

const QUALITIES: { id: Quality; label: string; hint: string }[] = [
  { id: "battery", label: "Battery", hint: "Fewer particles at 20 frames a second." },
  { id: "balanced", label: "Balanced", hint: "Smooth at 30 frames a second, or 24 on very large displays. Recommended." },
  { id: "best", label: "Best", hint: "Full resolution at 60 frames a second." },
];

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

function status(settings: Settings, desktop: DesktopStatus) {
  if (!desktop.gnome) return { text: "This desktop is not supported yet", good: false };
  if (!desktop.engine_installed) return { text: "Renderer not installed", good: false };
  if (desktop.extension === "needs_login") return { text: "Log out and back in to start", good: false };
  if (desktop.extension !== "active") return { text: "GNOME extension is off", good: false };
  return settings.paused ? { text: "Paused", good: false } : { text: "Running", good: true };
}

function Toggle(props: { label: string; hint: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <label className="flex cursor-pointer items-start justify-between gap-6 py-3">
      <span>
        <span className="block font-medium">{props.label}</span>
        <span className="mt-0.5 block text-sm text-muted">{props.hint}</span>
      </span>
      <input
        type="checkbox"
        role="switch"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
        className="peer sr-only"
      />
      <span
        aria-hidden="true"
        className={`mt-0.5 flex h-6 w-10 shrink-0 rounded-full p-0.5 transition-colors peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-accent ${props.checked ? "justify-end bg-accent" : "justify-start bg-line"}`}
      >
        <span className="block h-5 w-5 rounded-full bg-card" />
      </span>
    </label>
  );
}

function Slider(props: {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  shown: string;
  onChange: (value: number) => void;
}) {
  return (
    <div className="py-4">
      <div className="mb-2 flex justify-between gap-4">
        <label htmlFor={props.id} className="font-medium">
          {props.label}
        </label>
        <span className="text-muted tabular-nums">{props.shown}</span>
      </div>
      <input
        id={props.id}
        type="range"
        min={props.min}
        max={props.max}
        value={Math.round(props.value * 100)}
        onChange={(e) => props.onChange(Number(e.target.value) / 100)}
        className="w-full"
      />
    </div>
  );
}

function Choice<T extends string>(props: {
  label: string;
  hint?: string;
  value: T;
  options: { id: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 py-4">
      <div className="min-w-48 flex-1">
        <div className="font-medium">{props.label}</div>
        {props.hint && <div className="mt-0.5 text-sm text-muted">{props.hint}</div>}
      </div>
      <div role="radiogroup" aria-label={props.label} className="flex shrink-0 rounded-lg bg-page p-1">
        {props.options.map((o) => (
          <button
            key={o.id}
            type="button"
            role="radio"
            aria-checked={o.id === props.value}
            onClick={() => props.onChange(o.id)}
            className={`rounded-md px-3 py-1.5 text-sm font-medium ${o.id === props.value ? "bg-accent text-on-accent" : "text-muted hover:text-ink"}`}
          >
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}

const percent = (value: number) => `${Math.round(value * 100)}%`;

function windLabel(value: number) {
  if (Math.abs(value) < 0.03) return "Calm";
  return `${percent(Math.abs(value))} ${value < 0 ? "left" : "right"}`;
}

function Atmosphere({ state, update }: { state: AppState; update: Update }) {
  const { settings, auras } = state;
  const selected = auras.find((a) => a.id === settings.aura) ?? auras[0];
  const intensity = settings.intensity ?? selected.intensity;
  const { tuning } = settings;
  const tune = (change: Partial<Tuning>) => update({ tuning: { ...tuning, ...change } });
  const placement = PLACEMENTS.find((p) => p.id === placementOf(tuning.placement)) ?? PLACEMENTS[0];
  const snowy = selected.effect === "snow";
  return (
    <div className="space-y-7">
      <section aria-labelledby="aura-heading">
        <h2 id="aura-heading" className={heading}>
          Aura
        </h2>
        <div role="radiogroup" aria-labelledby="aura-heading" className="grid grid-cols-5 gap-3">
          {auras.map((aura) => {
            const on = aura.id === selected.id;
            return (
              <button
                key={aura.id}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => update({ aura: aura.id, intensity: null })}
                className={`overflow-hidden rounded-xl border-2 bg-card text-left ${on ? "border-accent" : "border-line hover:border-muted"}`}
              >
                <span className="block h-20 bg-[#26232a]">
                  <AuraPreview aura={aura} active={on} />
                </span>
                <span className="block truncate px-3 py-2 text-sm font-medium">{aura.name}</span>
              </button>
            );
          })}
        </div>
      </section>

      <section aria-labelledby="motion-heading">
        <h2 id="motion-heading" className={heading}>
          Look and motion
        </h2>
        <div className={panel}>
          <Slider id="intensity" label="Amount" value={intensity} min={0} max={100} shown={percent(intensity)} onChange={(value) => update({ intensity: value })} />
          <Slider id="speed" label="Speed" value={tuning.speed} min={25} max={250} shown={percent(tuning.speed)} onChange={(speed) => tune({ speed })} />
          <Slider id="size" label="Size" value={tuning.size} min={50} max={200} shown={percent(tuning.size)} onChange={(size) => tune({ size })} />
          <Slider id="wind" label="Wind" value={tuning.wind ?? 0} min={-100} max={100} shown={tuning.wind === null ? "Natural" : windLabel(tuning.wind)} onChange={(wind) => tune({ wind })} />
          {tuning.wind !== null && (
            <div className="py-3">
              <button type="button" onClick={() => tune({ wind: null })} className={quietButton}>
                Use natural wind
              </button>
            </div>
          )}
        </div>
      </section>

      <section aria-labelledby="behavior-heading">
        <h2 id="behavior-heading" className={heading}>
          Behavior
        </h2>
        <div className={panel}>
          <Choice label="Placement" hint={placement.hint} value={placement.id} options={PLACEMENTS} onChange={(value) => tune({ placement: value })} />
          <Toggle label="React to the pointer" hint="Leaves scatter, snow can be scooped and fireflies gather where it rests." checked={tuning.pointer} onChange={(pointer) => tune({ pointer })} />
          <Toggle label="Land on windows" hint="Effects settle on window tops and fall when a window moves." checked={tuning.windows} onChange={(windows) => tune({ windows })} />
          <Toggle label="Let it pile up" hint="Snow builds into drifts, and leaves and petals stay longer." checked={tuning.piles} onChange={(piles) => tune({ piles })} />
          {tuning.piles && snowy && (
            <Slider id="depth" label="Snow depth" value={tuning.pile_depth} min={0} max={100} shown={percent(tuning.pile_depth)} onChange={(pile_depth) => tune({ pile_depth })} />
          )}
          <Toggle label="Lightning" hint="Occasional bolts and a faint flash during Night Storm." checked={tuning.lightning} onChange={(lightning) => tune({ lightning })} />
        </div>
      </section>
    </div>
  );
}

function replaced<T>(items: T[], index: number, change: Partial<T>) {
  return items.map((item, i) => (i === index ? { ...item, ...change } : item));
}

function DecorationCard(props: {
  title: string;
  x: number;
  size: number;
  idPrefix: string;
  onChange: (change: { x?: number; size?: number }) => void;
  onRemove: () => void;
  children: ReactNode;
}) {
  return (
    <div className="py-2">
      <div className="flex items-center justify-between pt-3">
        <h3 className="font-semibold">{props.title}</h3>
        <button type="button" aria-label={`Remove ${props.title}`} onClick={props.onRemove} className="text-sm font-medium text-muted hover:underline">
          Remove
        </button>
      </div>
      <Slider id={`${props.idPrefix}-x`} label="Position" value={props.x} min={0} max={100} shown={percent(props.x)} onChange={(x) => props.onChange({ x })} />
      <Slider id={`${props.idPrefix}-size`} label="Size" value={props.size} min={50} max={160} shown={percent(props.size)} onChange={(size) => props.onChange({ size })} />
      {props.children}
    </div>
  );
}

function DecorationsPage({ state, update }: { state: AppState; update: Update }) {
  const { decorations } = state.settings;
  const change = (next: Partial<Decorations>) => update({ decorations: { ...decorations, ...next } });
  const { trees } = decorations;
  const setTree = (i: number, next: Partial<Tree>) => change({ trees: replaced(trees, i, next) });
  return (
    <div className="space-y-7">
      <p className="text-sm text-muted">Decorations stand along the bottom of your display and step aside when a window covers them.</p>
      <section aria-labelledby="trees-heading">
        <div className="mb-3 flex items-center justify-between">
          <h2 id="trees-heading" className="text-sm font-semibold text-muted">
            Trees
          </h2>
          <button type="button" disabled={trees.length >= MAX_DECORATIONS} onClick={() => change({ trees: [...trees, { ...NEW_TREE, x: 0.12 + trees.length * 0.1 }] })} className={`${quietButton} disabled:opacity-50`}>
            Add tree
          </button>
        </div>
        {trees.length > 0 && (
          <div className={panel}>
            {trees.map((tree, i) => (
              <DecorationCard key={i} title={`Tree ${i + 1}`} idPrefix={`tree-${i}`} x={tree.x} size={tree.size} onChange={(next) => setTree(i, next)} onRemove={() => change({ trees: trees.filter((_, n) => n !== i) })}>
                <Toggle label="Baubles" hint="Round ornaments." checked={tree.baubles} onChange={(baubles) => setTree(i, { baubles })} />
                <Toggle label="Lights" hint="Soft twinkling lights." checked={tree.lights} onChange={(lights) => setTree(i, { lights })} />
                <Toggle label="Candy canes" hint="Striped sweets on the branches." checked={tree.candy} onChange={(candy) => setTree(i, { candy })} />
                <Toggle label="Star" hint="A bright star on top." checked={tree.star} onChange={(star) => setTree(i, { star })} />
              </DecorationCard>
            ))}
          </div>
        )}
      </section>

      <section aria-labelledby="visitors-heading">
        <h2 id="visitors-heading" className={heading}>
          Visitors
        </h2>
        <div className={panel}>
          <Choice label="Sleigh" hint="Crosses the sky now and then in snowy auras." value={decorations.sleigh} options={FREQUENCIES} onChange={(sleigh) => change({ sleigh })} />
          <Choice label="Snowplough" hint="Clears deep snow, and pushes any snowman in its way." value={decorations.plough} options={FREQUENCIES} onChange={(plough) => change({ plough })} />
          <Choice
            label="Weather visitors"
            hint="In the snow a penguin, a fox or a snowy owl with a tiny present takes a turn. Other weather brings a frog, butterflies, a whirlwind, a comet, a heart of fireflies, a lantern or a paper plane."
            value={decorations.visitors ?? "sometimes"}
            options={FREQUENCIES}
            onChange={(visitors) => change({ visitors })}
          />
        </div>
        <p className="mt-2 text-sm text-muted">To call one right away, use the LarkAura menu in the top bar.</p>
      </section>
    </div>
  );
}

function WallpaperPage(props: { state: AppState; busy: boolean; update: Update; run: (task: () => Promise<AppState>) => void }) {
  const { state, run, busy, update } = props;
  const [moment, setMoment] = useState<string | null>(null);
  const first = state.wallpapers.find((w) => w.moments.length > 0);
  const options = [{ id: "day", label: "Through the day" }, ...(first?.moments ?? []).map((m) => ({ id: m.id, label: m.name }))];
  const add = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
    });
    if (typeof path === "string") run(() => api.importWallpaper(path));
  };
  return (
    <div>
      <div className="mb-4 flex items-center justify-between gap-4">
        <p role="status" className="text-sm text-muted">
          {busy ? "Preparing your wallpaper. This can take a few seconds." : "Wallpapers use no power once they are set."}
        </p>
        <div className="flex shrink-0 gap-2">
          {state.has_wallpaper_backup && (
            <button type="button" onClick={() => run(api.restoreWallpaper)} className={quietButton}>
              Restore my wallpaper
            </button>
          )}
          <button type="button" onClick={add} className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-on-accent">
            Add image
          </button>
        </div>
      </div>
      <div className={`${panel} mb-4`}>
        <Toggle
          label="Match the weather"
          hint="The wallpaper changes to a scene that suits each aura: a winter cabin for snow, a rainy village for rain, and so on."
          checked={state.settings.wallpaper_follows_aura ?? false}
          onChange={(wallpaper_follows_aura) => update({ wallpaper_follows_aura })}
        />
      </div>
      <div role="radiogroup" aria-label="Time of day" className="mb-4 flex flex-wrap gap-1 rounded-lg bg-card p-1">
        {options.map((o) => {
          const on = (moment ?? "day") === o.id;
          return (
            <button
              key={o.id}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => setMoment(o.id === "day" ? null : o.id)}
              className={`rounded-md px-3 py-1.5 text-sm font-medium ${on ? "bg-accent text-on-accent" : "text-muted hover:text-ink"}`}
            >
              {o.label}
            </button>
          );
        })}
      </div>
      <ul className="grid grid-cols-3 gap-4">
        {state.wallpapers.map((w) => {
          const shown = w.moments.find((m) => m.id === moment)?.preview ?? w.preview;
          return (
            <li key={w.id} className="overflow-hidden rounded-xl border border-line bg-card">
              <img src={imageUrl(shown)} alt="" loading="lazy" className="aspect-video w-full object-cover" />
              <div className="p-3">
                <div className="truncate text-sm font-medium" title={w.name}>
                  {w.name}
                </div>
                <div className="mt-0.5 text-xs text-muted">{w.imported ? "Your image" : moment ? "Stays at this time" : "Changes through the day"}</div>
                <div className="mt-2 flex gap-3 text-sm font-medium">
                  <button type="button" disabled={busy} aria-label={`Use ${w.name}`} onClick={() => run(() => api.applyWallpaper(w.id, w.imported ? null : moment))} className="text-accent hover:underline disabled:opacity-50">
                    Use
                  </button>
                  {w.imported && (
                    <button type="button" aria-label={`Remove ${w.name}`} onClick={() => run(() => api.removeWallpaper(w.id))} className="text-muted hover:underline">
                      Remove
                    </button>
                  )}
                </div>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function SystemPage(props: { state: AppState; update: Update; run: (task: () => Promise<AppState>) => void }) {
  const { desktop, settings } = props.state;
  const { update } = props;
  const quality = QUALITIES.find((q) => q.id === settings.quality) ?? QUALITIES[1];
  const battery = BATTERY.find((b) => b.id === settings.battery) ?? BATTERY[0];
  const extensionText: Record<DesktopStatus["extension"], string> = {
    active: "Running",
    disabled: "Turned off",
    needs_login: "Installed. Log out and back in to start it.",
    error: "GNOME reported an error",
    missing: "Not installed",
    unknown: "Could not be checked",
  };
  const rows = [
    ["Desktop", desktop.gnome ? "GNOME on Wayland" : "Not supported yet"],
    ["Renderer", desktop.engine_installed ? "Installed" : "Not installed"],
    ["GNOME extension", extensionText[desktop.extension]],
  ];
  return (
    <div className="space-y-7">
      <section aria-labelledby="performance-heading">
        <h2 id="performance-heading" className={heading}>
          Performance
        </h2>
        <div className={panel}>
          <Choice label="Quality" hint={quality.hint} value={settings.quality} options={QUALITIES} onChange={(value) => update({ quality: value })} />
          <Choice label="On battery" hint={battery.hint} value={settings.battery} options={BATTERY} onChange={(value) => update({ battery: value })} />
          <Toggle label="Stillness" hint="Effects fade while you work and return when you pause." checked={settings.stillness} onChange={(stillness) => update({ stillness })} />
        </div>
      </section>
      <section aria-labelledby="status-heading">
        <h2 id="status-heading" className={heading}>
          Status
        </h2>
        <div className="rounded-xl border border-line bg-card px-5">
      <dl className="divide-y divide-line">
        {rows.map(([label, value]) => (
          <div key={label} className="flex justify-between gap-6 py-3.5">
            <dt className="font-medium">{label}</dt>
            <dd className="text-right text-muted">{value}</dd>
          </div>
        ))}
      </dl>
      {desktop.extension === "disabled" && (
        <div className="border-t border-line py-4">
          <button type="button" onClick={() => props.run(api.enableExtension)} className="rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-on-accent">
            Turn on extension
          </button>
        </div>
      )}
        </div>
      </section>
    </div>
  );
}

export function App() {
  const [state, setState] = useState<AppState | null>(null);
  const [page, setPage] = useState<Page>("atmosphere");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    api.state().then(setState, (e) => setError(message(e)));
  }, []);

  const update = useCallback((change: Partial<Settings>) => {
    setState((current) => {
      if (!current) return current;
      const settings = { ...current.settings, ...change };
      window.clearTimeout(saveTimer.current);
      // A short delay keeps slider drags from rewriting the file constantly.
      saveTimer.current = window.setTimeout(() => {
        api.saveSettings(settings).then(() => setError(null), (e) => setError(message(e)));
      }, 120);
      return { ...current, settings };
    });
  }, []);

  const run = useCallback((task: () => Promise<AppState>) => {
    setBusy(true);
    task()
      .then(
        (next) => {
          setState(next);
          setError(null);
        },
        (e) => setError(message(e)),
      )
      .finally(() => setBusy(false));
  }, []);

  if (!state) {
    return <main className="p-8 text-muted">{error ?? "Loading"}</main>;
  }
  const current = status(state.settings, state.desktop);
  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-6 border-b border-line px-7 py-4">
        <h1 className="text-lg font-bold tracking-tight">LarkAura</h1>
        <nav aria-label="Sections" className="flex gap-1">
          {PAGES.map((p) => (
            <button
              key={p.id}
              type="button"
              aria-current={page === p.id ? "page" : undefined}
              onClick={() => setPage(p.id)}
              className={`rounded-lg px-3 py-1.5 text-sm font-medium ${page === p.id ? "bg-sand text-ink" : "text-muted hover:text-ink"}`}
            >
              {p.label}
            </button>
          ))}
        </nav>
        <div className="ml-auto flex items-center gap-4">
          <span role="status" className="flex items-center gap-2 text-sm text-muted">
            <span aria-hidden="true" className={`h-2 w-2 rounded-full ${current.good ? "bg-accent" : "bg-line"}`} />
            {current.text}
          </span>
          <button
            type="button"
            onClick={() => update({ paused: !state.settings.paused })}
            className="rounded-lg border border-line px-3 py-1.5 text-sm font-medium hover:border-muted"
          >
            {state.settings.paused ? "Resume" : "Pause"}
          </button>
        </div>
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
        {error && (
          <p role="alert" className="mb-4 rounded-lg bg-sand px-4 py-2.5 text-sm">
            {error}
          </p>
        )}
        {page === "atmosphere" && <Atmosphere state={state} update={update} />}
        {page === "decorations" && <DecorationsPage state={state} update={update} />}
        {page === "wallpaper" && <WallpaperPage state={state} busy={busy} update={update} run={run} />}
        {page === "system" && <SystemPage state={state} update={update} run={run} />}
      </main>
    </div>
  );
}
