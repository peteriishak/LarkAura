// LarkAura for GNOME Shell.
//
// This extension only places and informs the renderer. All drawing happens in
// a separate process, so a renderer fault can never take GNOME Shell down.

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

const PROTOCOL_VERSION = 1;
const TICK_MS = 150;
// While the pointer or a window is moving, effects need fresher positions.
const BUSY_TICK_MS = 50;
// During snow, the pointer button is checked this often, so a short click on
// the snow is not missed between ticks. Reading it is cheap.
const BUTTON_POLL_MS = 30;
const SNOW_AURAS = new Set(['quiet-snow', 'deep-snow']);
const RESTART_DELAYS_S = [1, 5, 30];
const RESTART_WINDOW_S = 600;
const SIGTERM = 15;
const QUALITIES = ['battery', 'balanced', 'best'];
const BATTERY_MODES = ['reduce', 'pause', 'ignore'];
const STUDIO_APP = 'dev.larklabs.LarkAura.Studio.desktop';
const AURAS = [
    ['first-light', 'First Light'],
    ['quiet-snow', 'Quiet Snow'],
    ['deep-snow', 'Deep Snow'],
    ['soft-rain', 'Soft Rain'],
    ['night-storm', 'Night Storm'],
    ['autumn-leaves', 'Autumn Leaves'],
    ['blossom-drift', 'Blossom Drift'],
    ['falling-stars', 'Falling Stars'],
    ['fireflies', 'Fireflies'],
    ['low-fog', 'Low Fog'],
];
// The menu keeps to the auras people reach for most. Each entry covers the
// auras it stands for, so it shows as selected for either of them; the first
// is the one it switches to. The others stay a click away in Studio.
const MENU_AURAS = [
    ['Snow', 'aura-snow', ['deep-snow', 'quiet-snow']],
    ['Rain', 'aura-rain', ['soft-rain', 'night-storm']],
    ['Autumn Leaves', 'aura-leaves', ['autumn-leaves']],
    ['Blossom', 'aura-blossom', ['blossom-drift']],
    ['Fireflies', 'aura-fireflies', ['fireflies']],
    ['Falling Stars', 'aura-stars', ['falling-stars']],
];
// Quick actions, shown as a row of icons: [icon, name, event, snow only].
const ACTIONS = [
    ['visit-sleigh', 'Send the Sleigh', 'sleigh', true],
    ['visit-plough', 'Send the Snowplough', 'plough', true],
    ['visit-critter', 'Send a Visitor', 'visitor', false],
    ['visit-shake', 'Shake Off Snow and Leaves', 'shake_off', false],
];
const DEFAULTS = {
    aura: 'first-light',
    intensity: null,
    quality: 'balanced',
    stillness: true,
    paused: false,
    battery: 'reduce',
    tuning: {},
    decorations: {},
    displays_off: [],
    wallpaper_follows_aura: false,
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// Mutter aborts if window state changes while a frame is being painted, and
// several signals fire during paint, so window changes always wait for idle.
function later(fn) {
    return GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
        fn();
        return GLib.SOURCE_REMOVE;
    });
}

function sanitize(raw) {
    const s = {...DEFAULTS};
    if (typeof raw?.aura === 'string' && /^[a-z0-9-]{1,48}$/.test(raw.aura))
        s.aura = raw.aura;
    if (typeof raw?.intensity === 'number' && raw.intensity >= 0 && raw.intensity <= 1)
        s.intensity = raw.intensity;
    if (QUALITIES.includes(raw?.quality))
        s.quality = raw.quality;
    for (const key of ['stillness', 'paused', 'wallpaper_follows_aura']) {
        if (typeof raw?.[key] === 'boolean')
            s[key] = raw[key];
    }
    if (BATTERY_MODES.includes(raw?.battery))
        s.battery = raw.battery;
    else if (raw?.pause_on_battery === true)
        s.battery = 'pause';
    // The renderer validates these in full and ignores anything it rejects.
    for (const key of ['tuning', 'decorations']) {
        if (raw?.[key] && typeof raw[key] === 'object' && !Array.isArray(raw[key]))
            s[key] = raw[key];
    }
    if (Array.isArray(raw?.displays_off))
        s.displays_off = raw.displays_off.filter(n => Number.isInteger(n) && n >= 0);
    return s;
}

// Writes never block GNOME Shell. While one write is in flight, newer messages
// replace older ones of the same kind, so a stuck renderer cannot grow a queue.
class CommandPipe {
    constructor(stream) {
        this._stream = stream;
        this._cancellable = new Gio.Cancellable();
        this._pending = new Map();
        this._writing = false;
    }

    send(message) {
        const key = message.cmd === 'trigger' ? `trigger:${message.event}` : message.cmd;
        this._pending.set(key, message);
        this._flush();
    }

    _flush() {
        if (this._writing || this._pending.size === 0 || this._cancellable.is_cancelled())
            return;
        const text = [...this._pending.values()].map(m => `${JSON.stringify(m)}\n`).join('');
        this._pending.clear();
        this._writing = true;
        this._write(encoder.encode(text));
    }

    _write(data) {
        this._stream.write_bytes_async(new GLib.Bytes(data), GLib.PRIORITY_DEFAULT,
            this._cancellable, (stream, result) => {
                try {
                    const written = stream.write_bytes_finish(result);
                    if (written < data.length) {
                        this._write(data.subarray(written));
                        return;
                    }
                } catch (e) {
                    if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        console.debug(`LarkAura: renderer pipe closed: ${e.message}`);
                    this._writing = false;
                    return;
                }
                this._writing = false;
                this._flush();
            });
    }

    close() {
        this._cancellable.cancel();
        this._stream.close_async(GLib.PRIORITY_DEFAULT, null, (stream, result) => {
            try {
                stream.close_finish(result);
            } catch {
                // The renderer has already gone.
            }
        });
    }
}

// One renderer process and its window, covering one monitor's work area.
class MonitorOverlay {
    constructor(owner, monitor) {
        this._owner = owner;
        this.monitor = monitor;
        this._restarts = [];
        this._retryId = 0;
        this._lastKey = null;
        this._lastIdle = null;
        this._lastTick = 0;
        this._lastAreaKey = null;
        this._clone = null;
        this._cloneSource = null;
        this._actorDestroyId = 0;
        this.start();
    }

    start() {
        const engine = this._owner.enginePath;
        if (!engine)
            return;
        const s = this._owner.settings;
        const argv = [engine, '--mode', 'gnome', '--control', '--aura', s.aura, '--quality', s.quality];
        if (s.intensity !== null)
            argv.push('--intensity', String(s.intensity));
        if (!s.stillness)
            argv.push('--no-stillness');
        try {
            const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.STDIN_PIPE});
            this._client = Meta.WaylandClient.new_subprocess(global.context, launcher, argv);
            this._process = this._client.get_subprocess();
            this._pipe = new CommandPipe(this._process.get_stdin_pipe());
        } catch (e) {
            console.warn(`LarkAura: could not start the renderer: ${e.message}`);
            this._scheduleRestart();
            return;
        }
        this._lastKey = null;
        this._lastIdle = null;
        this._pipe.send({cmd: 'hello', version: PROTOCOL_VERSION});
        this.applySettings();
        const process = this._process;
        this._waitCancel = new Gio.Cancellable();
        process.wait_async(this._waitCancel, (p, result) => {
            try {
                p.wait_finish(result);
            } catch {
                return; // Cancelled because we stopped it ourselves.
            }
            if (process === this._process)
                this._onExit();
        });
    }

    _onExit() {
        this._dropProcess();
        this._hideWindow();
        this._scheduleRestart();
    }

    // GNOME can keep a dead client's window for several seconds. Hiding it
    // immediately stops the renderer's last frame from staying on screen.
    _hideWindow() {
        this._dropClone();
        const actor = this.window?.get_compositor_private();
        if (actor) {
            Main.wm.skipNextEffect(actor);
            actor.hide();
        }
        this.window = null;
    }

    _dropClone() {
        const actor = this.window?.get_compositor_private();
        if (actor && this._actorDestroyId) {
            try {
                actor.disconnect(this._actorDestroyId);
            } catch {
                // Actor already disposed.
            }
        }
        this._actorDestroyId = 0;
        if (actor) {
            try {
                actor.remove_clip();
            } catch {
                // Actor already disposed.
            }
        }
        const clone = this._clone;
        this._clone = null;
        this._cloneSource = null;
        if (clone) {
            try {
                clone.destroy();
            } catch {
                // Clone already disposed during shell shutdown.
            }
        }
    }

    // When a GNOME Shell dock (such as Dash-to-Dock) sits in uiGroup above
    // window_group, paint a click-through clone of our overlay surface directly
    // above the dock while clipping the underlying WindowActor so it renders
    // once — in front of the dock, below popups/overview, and 100% click-through.
    _syncDockClone() {
        const actor = this.window?.get_compositor_private();
        const chromeDocks = this._owner.docksForMonitor(this.monitor)
            .map(d => d.chrome)
            .filter(c => {
                if (!c)
                    return false;
                try {
                    return c.get_parent() === Main.uiGroup;
                } catch {
                    return false;
                }
            });
        const topDock = chromeDocks.at(-1) ?? null;
        let surface = null;
        try {
            surface = actor?.get_first_child() ?? null;
        } catch {
            surface = null;
        }
        const wantClone = Boolean(
            actor && surface && topDock && !this._behind && !this._owner.settings?.paused
        );
        if (!wantClone) {
            this._dropClone();
            return;
        }
        if (!this._clone || this._cloneSource !== surface) {
            this._dropClone();
            const clone = new Clutter.Clone({
                source: surface,
                reactive: false,
            });
            clone.connect('destroy', () => {
                if (this._clone === clone) {
                    this._clone = null;
                    this._cloneSource = null;
                }
            });
            this._clone = clone;
            this._cloneSource = surface;
            this._actorDestroyId = actor.connect('destroy', () => {
                this._actorDestroyId = 0;
                const c = this._clone;
                this._clone = null;
                this._cloneSource = null;
                try {
                    c?.destroy();
                } catch {
                    // Clone already disposed.
                }
            });
            Main.uiGroup.insert_child_above(this._clone, topDock);
        }
        const a = this.area;
        this._clone.set_position(a.x, a.y);
        this._clone.set_size(a.width, a.height);
        if (this._clone.get_parent() === Main.uiGroup && topDock.get_parent() === Main.uiGroup)
            Main.uiGroup.set_child_above_sibling(this._clone, topDock);
        const show = !Main.overview.visible && !global.display.get_monitor_in_fullscreen(this.monitor);
        this._clone.visible = show;
        actor.set_clip(0, 0, 0, 0);
    }

    _scheduleRestart() {
        const now = GLib.get_monotonic_time() / 1e6;
        this._restarts = this._restarts.filter(t => now - t < RESTART_WINDOW_S);
        const delay = RESTART_DELAYS_S[this._restarts.length];
        if (delay === undefined) {
            console.warn('LarkAura: the renderer keeps stopping, so effects are off for now');
            return;
        }
        this._restarts.push(now);
        this._retryId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, delay, () => {
            this._retryId = 0;
            this.start();
            return GLib.SOURCE_REMOVE;
        });
    }

    _dropProcess() {
        this._waitCancel?.cancel();
        this._waitCancel = null;
        this._pipe?.close();
        this._pipe = null;
        this._process = null;
        this._client = null;
    }

    owns(win) {
        return this._client?.owns_window(win) ?? false;
    }

    adopt(win) {
        this.window = win;
        const actor = win.get_compositor_private();
        Main.wm.skipNextEffect(actor);
        // window-created fires before the window joins the stacking order.
        later(() => {
            if (this.window === win) {
                win.set_type(Meta.WindowType.DOCK);
                win.hide_from_window_list();
            }
        });
        const id = actor.connect('first-frame', () => {
            actor.disconnect(id);
            later(() => {
                if (this.window === win)
                    this.place();
            });
        });
    }

    get area() {
        const wa = Main.layoutManager.getWorkAreaForMonitor(this.monitor);
        const mon = Main.layoutManager.monitors[this.monitor];
        if (!mon || this._owner.docksForMonitor(this.monitor).length === 0)
            return wa;
        // Extend down to the monitor's bottom edge (keeping the top panel clear)
        // so snow falls in front of the dock and accumulates along its top edge.
        const top = Math.max(mon.y, wa.y);
        return {
            x: mon.x,
            y: top,
            width: mon.width,
            height: Math.max(1, mon.y + mon.height - top),
        };
    }

    place() {
        const win = this.window;
        if (!win)
            return;
        const a = this.area;
        this._lastAreaKey = `${a.x},${a.y},${a.width},${a.height}`;
        win.stick();
        if (this._behind)
            win.unmake_above();
        else
            win.make_above();
        win.move_resize_frame(false, a.x, a.y, a.width, a.height);
        this.raise();
    }

    get _behind() {
        return this._owner.settings.tuning.placement === 'behind_windows';
    }

    // Keeps the effects above every window (and above dock chrome), or directly
    // above the wallpaper when placed behind windows.
    raise() {
        const actor = this.window?.get_compositor_private();
        const group = global.window_group;
        if (!actor || actor.get_parent() !== group)
            return;
        if (this._behind) {
            this._dropClone();
            const wallpaper = Main.layoutManager._backgroundGroup;
            if (wallpaper.get_parent() === group && actor.get_previous_sibling() !== wallpaper)
                group.set_child_above_sibling(actor, wallpaper);
        } else {
            if (group.get_last_child() !== actor)
                group.set_child_above_sibling(actor, null);
            this._syncDockClone();
        }
    }

    send(message) {
        this._pipe?.send(message);
    }

    applySettings() {
        const s = this._owner.settings;
        this._pipe?.send({cmd: 'set_aura', id: s.aura});
        if (s.intensity !== null)
            this._pipe?.send({cmd: 'set_intensity', value: s.intensity});
        this._pipe?.send({cmd: 'set_quality', quality: s.quality});
        this._pipe?.send({cmd: 'set_stillness', enabled: s.stillness});
        this._pipe?.send({cmd: 'set_paused', paused: s.paused});
        this._pipe?.send({cmd: 'set_battery', mode: s.battery});
        this._pipe?.send({cmd: 'set_tuning', tuning: s.tuning});
        this._pipe?.send({cmd: 'set_decorations', decorations: s.decorations});
        if (this.window) {
            later(() => {
                if (this.window)
                    this.place();
            });
        }
    }

    // Returns true when something on this display changed.
    tick(snapshot, nowMs) {
        if (!this._pipe)
            return false;
        const a = this.area;
        const areaKey = `${a.x},${a.y},${a.width},${a.height}`;
        const hasChromeDock = snapshot.docks.some(d => d.monitor === this.monitor && d.chrome);
        const wantClone = hasChromeDock && !this._behind && !this._owner.settings.paused;
        if (this.window && (areaKey !== this._lastAreaKey || wantClone !== Boolean(this._clone))) {
            later(() => {
                if (this.window)
                    this.place();
            });
        } else if (this._clone) {
            this._syncDockClone();
        }
        const local = r => ({x: r.x - a.x, y: r.y - a.y, w: r.width, h: r.height});
        const overlaps = r => r.x < a.x + a.width && r.x + r.width > a.x &&
            r.y < a.y + a.height && r.y + r.height > a.y;
        const [px, py] = snapshot.pointer;
        const inside = px >= a.x && px < a.x + a.width && py >= a.y && py < a.y + a.height;
        const desktop = {
            cmd: 'desktop',
            windows: snapshot.windows.filter(w => overlaps(w.rect))
                .map(w => ({id: w.id, ...local(w.rect)})),
            docks: snapshot.docks.filter(d => d.monitor === this.monitor && overlaps(d.rect))
                .map(d => ({id: d.id, ...local(d.rect)})),
            focused: snapshot.focused && overlaps(snapshot.focused) ? local(snapshot.focused) : null,
            cursor: inside ? [px - a.x, py - a.y] : null,
            pointer_down: inside && snapshot.pointerDown,
            fullscreen: global.display.get_monitor_in_fullscreen(this.monitor),
            on_battery: snapshot.onBattery,
        };
        const key = JSON.stringify(desktop);
        // The renderer counts idle time itself, so only activity needs reporting.
        const expected = this._lastIdle === null ? null : this._lastIdle + (nowMs - this._lastTick);
        const active = expected === null || snapshot.idle < expected - TICK_MS;
        this._lastIdle = snapshot.idle;
        this._lastTick = nowMs;
        const changed = key !== this._lastKey;
        if (changed || active) {
            this._lastKey = key;
            this._pipe.send({...desktop, idle_ms: snapshot.idle});
        }
        return changed;
    }

    destroy() {
        if (this._retryId)
            GLib.source_remove(this._retryId);
        this._retryId = 0;
        const process = this._process;
        this._dropProcess();
        process?.send_signal(SIGTERM);
        this._hideWindow();
    }
}

// The top bar menu. GNOME has no system tray, so this is the quick control.
const Indicator = GObject.registerClass(
class LarkAuraIndicator extends PanelMenu.Button {
    _init(owner) {
        super._init(0.5, 'LarkAura');
        this._owner = owner;
        this.menu.actor.add_style_class_name('larkaura-menu');
        this.menu.box.add_style_class_name('larkaura-menu-box');
        this.add_child(new St.Icon({
            gicon: this._icon('larkaura'),
            style_class: 'system-status-icon',
        }));
        this._effects = new PopupMenu.PopupSwitchMenuItem('Effects', true);
        this._effects.connect('toggled', (_item, on) => owner.changeSettings({paused: !on}));
        this.menu.addMenuItem(this._effects);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        // An aura chosen in Studio that the short list leaves out still
        // shows here, selected, so the menu always says what is running.
        this._other = new PopupMenu.PopupImageMenuItem('', this._icon('aura-other'));
        this._other.add_style_class_name('larkaura-aura');
        this.menu.addMenuItem(this._other);
        this._auras = MENU_AURAS.map(([name, icon, ids]) => {
            const item = new PopupMenu.PopupImageMenuItem(name, this._icon(icon));
            item.add_style_class_name('larkaura-aura');
            item.connect('activate', () => {
                if (!ids.includes(this._owner.settings.aura))
                    owner.changeSettings({aura: ids[0], intensity: null});
            });
            this.menu.addMenuItem(item);
            return [ids, item];
        });

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addMenuItem(this._actionRow(owner));
        this._follow = new PopupMenu.PopupSwitchMenuItem('Wallpaper Matches the Weather', false);
        this._follow.connect('toggled', (_item, on) => owner.changeSettings({wallpaper_follows_aura: on}));
        this.menu.addMenuItem(this._follow);
        this._displaySection = new PopupMenu.PopupMenuSection();
        this.menu.addMenuItem(this._displaySection);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addAction('Open LarkAura Studio', () => owner.openStudio());
        this.sync(owner.settings);
    }

    _icon(name) {
        return Gio.icon_new_for_string(`${this._owner.path}/icons/${name}-symbolic.svg`);
    }

    // One compact row of round icon buttons with a full-width description line
    // directly beneath them, so long action names never widen or clip the menu.
    _actionRow(owner) {
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        row.add_style_class_name('larkaura-actions');
        const wrap = new St.BoxLayout({
            style_class: 'larkaura-action-wrap',
            vertical: true,
            x_expand: true,
        });
        const box = new St.BoxLayout({style_class: 'larkaura-action-box', x_expand: true});
        const hint = new St.Label({
            style_class: 'larkaura-action-hint',
            text: '',
            x_expand: true,
            x_align: Clutter.ActorAlign.START,
            y_align: Clutter.ActorAlign.CENTER,
        });
        hint.clutter_text.line_wrap = true;
        hint.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
        hint.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        hint.clutter_text.single_line_mode = false;
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (!open)
                hint.text = '';
        });
        this._actions = ACTIONS.map(([icon, name, event, snowOnly]) => {
            const button = new St.Button({
                style_class: 'icon-button larkaura-action',
                can_focus: true,
                track_hover: true,
                accessible_name: name,
                child: new St.Icon({gicon: this._icon(icon)}),
            });
            button.connect('clicked', () => {
                owner.sendEvent(event);
                this.menu.close();
            });
            const show = () => {
                hint.text = button.hover || button.has_key_focus() ? name : hint.text;
            };
            const clear = () => {
                if (!button.hover && !button.has_key_focus() && hint.text === name)
                    hint.text = '';
            };
            button.connect('notify::hover', () => (button.hover ? show() : clear()));
            button.connect('key-focus-in', show);
            button.connect('key-focus-out', clear);
            box.add_child(button);
            return [button, snowOnly];
        });
        wrap.add_child(box);
        wrap.add_child(hint);
        row.add_child(wrap);
        return row;
    }

    // Each display gets its own switch, once there is more than one. The items
    // are rebuilt only when the displays change, because a switch that is being
    // toggled must not be destroyed by the settings change it causes.
    _syncDisplays(settings) {
        const monitors = Main.layoutManager.monitors;
        const layout = monitors.map(m => `${m.width}x${m.height}`).join();
        if (layout === this._displayLayout) {
            this._displayItems.forEach((item, index) =>
                item.setToggleState(!settings.displays_off.includes(index)));
            return;
        }
        this._displayLayout = layout;
        this._displayItems = [];
        this._displaySection.removeAll();
        if (monitors.length < 2)
            return;
        this._displaySection.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        monitors.forEach((monitor, index) => {
            const label = `Display ${index + 1} (${monitor.width} x ${monitor.height})`;
            const item = new PopupMenu.PopupSwitchMenuItem(label, !settings.displays_off.includes(index));
            this._displayItems.push(item);
            item.connect('toggled', (_item, on) => {
                const others = this._owner.settings.displays_off.filter(n => n !== index);
                this._owner.changeSettings({displays_off: on ? others : [...others, index]});
            });
            this._displaySection.addMenuItem(item);
        });
    }

    _select(item, selected) {
        item.setOrnament(selected ? PopupMenu.Ornament.CHECK : PopupMenu.Ornament.NONE);
        if (selected)
            item.add_style_pseudo_class('checked');
        else
            item.remove_style_pseudo_class('checked');
    }

    sync(settings) {
        this._effects.setToggleState(!settings.paused);
        this._follow.setToggleState(settings.wallpaper_follows_aura);
        this._syncDisplays(settings);
        let listed = false;
        for (const [ids, item] of this._auras) {
            const selected = ids.includes(settings.aura);
            listed ||= selected;
            this._select(item, selected);
        }
        const other = AURAS.find(([id]) => id === settings.aura);
        this._other.visible = !listed && other !== undefined;
        if (this._other.visible) {
            this._other.label.text = other[1];
            this._select(this._other, true);
        }
        const snow = SNOW_AURAS.has(settings.aura);
        for (const [button, snowOnly] of this._actions)
            button.visible = snow || !snowOnly;
    }
});

export default class LarkAuraExtension extends Extension {
    enable() {
        this._settingsFile = Gio.File.new_for_path(
            GLib.build_filenamev([GLib.get_user_config_dir(), 'larkaura', 'settings.json']));
        this.settings = this._readSettings();
        this.enginePath = this._findEngine();
        if (!this.enginePath)
            console.warn('LarkAura: the renderer is not installed');
        this._overlays = [];
        this._docks = [];
        this._dockIds = new WeakMap();
        this._nextDockId = 0xD0CC0001;
        this._onBattery = false;
        this._signals = [
            [global.display, global.display.connect('window-created', (_d, win) => this._onWindow(win))],
            [global.display, global.display.connect('restacked', () => this._onRestacked())],
            [Main.layoutManager, Main.layoutManager.connect('monitors-changed', () => {
                this._rebuild();
                this._indicator?.sync(this.settings);
            })],
            [global.display, global.display.connect('workareas-changed', () => this._placeSoon())],
            [Main.overview, Main.overview.connect('showing', () => this._raiseSoon())],
            [Main.overview, Main.overview.connect('hidden', () => this._raiseSoon())],
        ];
        this._monitor = this._settingsFile.monitor_file(Gio.FileMonitorFlags.NONE, null);
        this._monitor.set_rate_limit(250);
        this._monitorId = this._monitor.connect('changed', () => this._reloadSettings());
        this._watchBattery();
        this._docks = this._findDocks();
        this._rebuild();
        this._indicator = new Indicator(this);
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        this._busy = false;
        this._scheduleTick();
    }

    _scheduleTick() {
        const delay = this._busy && !this.settings.paused ? BUSY_TICK_MS : TICK_MS;
        const watch = this._watchesButton();
        this._tickId = GLib.timeout_add(GLib.PRIORITY_LOW, watch ? BUTTON_POLL_MS : delay, () => {
            const waited = GLib.get_monotonic_time() / 1000 - (this._lastTickMs ?? 0);
            if (!watch || waited >= delay - BUTTON_POLL_MS / 2 || this._buttonChanged())
                this._tick();
            this._scheduleTick();
            return GLib.SOURCE_REMOVE;
        });
    }

    // Snow is the only effect that reacts to the pointer button.
    _watchesButton() {
        const s = this.settings;
        return !s.paused && SNOW_AURAS.has(s.aura) && s.tuning?.pointer !== false;
    }

    _buttonChanged() {
        const [, , mods] = global.get_pointer();
        return ((mods & Clutter.ModifierType.BUTTON1_MASK) !== 0) !== this._pointerDown;
    }

    disable() {
        GLib.source_remove(this._tickId);
        this._tickId = 0;
        this._indicator.destroy();
        this._indicator = null;
        this._signals.forEach(([object, id]) => object.disconnect(id));
        this._signals = [];
        this._monitor.disconnect(this._monitorId);
        this._monitor.cancel();
        this._monitor = null;
        this._batteryCancel.cancel();
        if (this._batteryProxy && this._batteryId)
            this._batteryProxy.disconnect(this._batteryId);
        this._batteryProxy = null;
        this._docks = [];
        this._overlays.forEach(o => o.destroy());
        this._overlays = [];
        this._holdBackgroundMenu(false);
        this.settings = null;
    }

    // Holding the left button on the wallpaper opens GNOME's background menu,
    // which would pop up over a snowball being packed. While snow reacts to
    // the button, that long press is switched off; right-click still opens
    // the menu. Only gestures switched off here are switched back on.
    _holdBackgroundMenu(hold) {
        this._heldGestures ??= new Set();
        if (hold) {
            // Forget gestures whose background has since been replaced.
            for (const action of this._heldGestures) {
                if (!action.get_actor())
                    this._heldGestures.delete(action);
            }
            for (const manager of Main.layoutManager._bgManagers ?? []) {
                for (const action of manager.backgroundActor?.get_actions() ?? []) {
                    if (action instanceof Clutter.LongPressGesture && action.enabled) {
                        action.enabled = false;
                        this._heldGestures.add(action);
                    }
                }
            }
        } else {
            for (const action of this._heldGestures) {
                try {
                    action.enabled = true;
                } catch {
                    // Its background was replaced and the gesture is gone.
                }
            }
            this._heldGestures.clear();
        }
    }

    _findEngine() {
        const candidates = [
            GLib.getenv('LARKAURA_ENGINE'),
            GLib.build_filenamev([GLib.get_home_dir(), '.local', 'bin', 'larkaura-engine']),
            GLib.find_program_in_path('larkaura-engine'),
        ];
        return candidates.find(p => p && GLib.file_test(p, GLib.FileTest.IS_EXECUTABLE)) ?? null;
    }

    _readSettings() {
        try {
            const [, contents] = this._settingsFile.load_contents(null);
            return sanitize(JSON.parse(decoder.decode(contents)));
        } catch (e) {
            if (!e.matches?.(Gio.IOErrorEnum, Gio.IOErrorEnum.NOT_FOUND))
                console.warn(`LarkAura: ignoring unreadable settings: ${e.message}`);
            return sanitize(null);
        }
    }

    _reloadSettings() {
        this._useSettings(this._readSettings());
    }

    _useSettings(settings) {
        const displaysChanged =
            JSON.stringify(settings.displays_off) !== JSON.stringify(this.settings.displays_off);
        const followNow = settings.wallpaper_follows_aura &&
            (!this.settings.wallpaper_follows_aura || settings.aura !== this.settings.aura);
        this.settings = settings;
        if (followNow)
            this._followWallpaper(settings.aura);
        if (displaysChanged)
            this._rebuild();
        else
            this._overlays.forEach(o => o.applySettings());
        this._indicator?.sync(settings);
    }

    // Sets a wallpaper that suits the aura. The helper draws the scenes the
    // first time, which takes a few seconds, so it runs in its own process and
    // only the newest request is kept.
    _followWallpaper(aura) {
        const helper = [
            GLib.build_filenamev([GLib.get_home_dir(), '.local', 'bin', 'larkaura-wallpaper']),
            GLib.find_program_in_path('larkaura-wallpaper'),
        ].find(p => p && GLib.file_test(p, GLib.FileTest.IS_EXECUTABLE));
        if (!helper) {
            console.warn('LarkAura: the wallpaper helper is not installed');
            return;
        }
        if (this._wallpaperJob) {
            this._wallpaperNext = aura;
            return;
        }
        try {
            const proc = Gio.Subprocess.new([helper, 'follow', aura],
                Gio.SubprocessFlags.STDOUT_SILENCE | Gio.SubprocessFlags.STDERR_PIPE);
            this._wallpaperJob = proc;
            proc.communicate_utf8_async(null, null, (p, result) => {
                try {
                    const [, , stderr] = p.communicate_utf8_finish(result);
                    if (!p.get_successful())
                        console.warn(`LarkAura: ${stderr?.trim() || 'could not set the wallpaper'}`);
                } catch (e) {
                    console.warn(`LarkAura: wallpaper helper failed: ${e.message}`);
                }
                this._wallpaperJob = null;
                const next = this._wallpaperNext;
                this._wallpaperNext = null;
                if (next && this.settings?.wallpaper_follows_aura)
                    this._followWallpaper(next);
            });
        } catch (e) {
            this._wallpaperJob = null;
            console.warn(`LarkAura: could not start the wallpaper helper: ${e.message}`);
        }
    }

    // Visitors appear on the main display. Shaking applies everywhere.
    sendEvent(event) {
        const message = {cmd: 'trigger', event};
        if (event === 'shake_off') {
            this._overlays.forEach(o => o.send(message));
            return;
        }
        const primary = Main.layoutManager.primaryIndex;
        (this._overlays.find(o => o.monitor === primary) ?? this._overlays[0])?.send(message);
    }

    // Applies a change from the menu at once, then saves it without blocking.
    changeSettings(change) {
        let saved = {};
        try {
            const [, contents] = this._settingsFile.load_contents(null);
            saved = JSON.parse(decoder.decode(contents));
        } catch {
            // Missing or damaged settings are replaced below.
        }
        const merged = {...saved, ...change};
        this._useSettings(sanitize(merged));
        GLib.mkdir_with_parents(this._settingsFile.get_parent().get_path(), 0o755);
        const bytes = new GLib.Bytes(encoder.encode(JSON.stringify(merged, null, 2)));
        this._settingsFile.replace_contents_bytes_async(bytes, null, false,
            Gio.FileCreateFlags.REPLACE_DESTINATION, null, (file, result) => {
                try {
                    file.replace_contents_finish(result);
                } catch (e) {
                    console.warn(`LarkAura: could not save settings: ${e.message}`);
                }
            });
    }

    openStudio() {
        const app = Shell.AppSystem.get_default().lookup_app(STUDIO_APP);
        if (app)
            app.activate();
        else
            Main.notify('LarkAura', 'LarkAura Studio is not installed.');
    }

    _watchBattery() {
        this._batteryCancel = new Gio.Cancellable();
        Gio.DBusProxy.new_for_bus(Gio.BusType.SYSTEM, Gio.DBusProxyFlags.DO_NOT_AUTO_START, null,
            'org.freedesktop.UPower', '/org/freedesktop/UPower', 'org.freedesktop.UPower',
            this._batteryCancel, (_source, result) => {
                try {
                    this._batteryProxy = Gio.DBusProxy.new_for_bus_finish(result);
                } catch {
                    return; // No power service, so treat the machine as plugged in.
                }
                const read = () => {
                    this._onBattery =
                        this._batteryProxy?.get_cached_property('OnBattery')?.get_boolean() ?? false;
                };
                this._batteryId = this._batteryProxy.connect('g-properties-changed', read);
                read();
            });
    }

    _rebuild() {
        this._overlays.forEach(o => o.destroy());
        const off = this.settings.displays_off;
        this._overlays = Main.layoutManager.monitors
            .map((_m, i) => i)
            .filter(i => !off.includes(i))
            .map(i => new MonitorOverlay(this, i));
    }

    // Effects behind windows must return before the next frame is drawn. This
    // only reorders actors, so it is safe to do during the signal.
    _onRestacked() {
        if (this.settings.tuning.placement === 'behind_windows')
            this._overlays.forEach(o => o.raise());
        else
            this._raiseSoon();
    }

    _onWindow(win) {
        this._overlays.find(o => o.owns(win))?.adopt(win);
    }

    _raiseSoon() {
        if (this._raiseQueued)
            return;
        this._raiseQueued = true;
        later(() => {
            this._raiseQueued = false;
            this._overlays?.forEach(o => o.raise());
        });
    }

    _placeSoon() {
        later(() => this._overlays?.forEach(o => o.place()));
    }

    docksForMonitor(monitor) {
        return this._docks.filter(d => d.monitor === monitor);
    }

    _dockId(actor) {
        let id = this._dockIds.get(actor);
        if (!id) {
            id = this._nextDockId++;
            this._dockIds.set(actor, id);
        }
        return id;
    }

    _findDockBar(actor) {
        if (actor.has_style_class_name?.('dash-background'))
            return actor;
        for (const child of actor.get_children?.() ?? []) {
            if (!child.visible)
                continue;
            const found = this._findDockBar(child);
            if (found)
                return found;
        }
        return null;
    }

    // Finds visible bottom/floating docks (Dash-to-Dock, bottom panels, or
    // external Wayland DOCK windows) so snow renders above them and piles along
    // their top edges.
    _findDocks() {
        const docks = [];
        const monitors = Main.layoutManager.monitors;
        const panel = Main.layoutManager.panelBox;
        for (const child of Main.uiGroup.get_children()) {
            try {
                if (!child.visible || !child.mapped || child.opacity === 0)
                    continue;
            } catch {
                continue;
            }
            const name = child.name ?? '';
            const isDashToDock = name === 'dashtodockContainer';
            const isBottomPanel = child === panel ||
                /dock|dash|panel|window-list/i.test(name);
            if (!isDashToDock && !isBottomPanel)
                continue;
            const bar = isDashToDock ? (this._findDockBar(child) ?? child) : child;
            if (!bar.visible || !bar.mapped || bar.opacity === 0)
                continue;
            const [x, y] = bar.get_transformed_position();
            const [w, h] = bar.get_transformed_size();
            if (![x, y, w, h].every(Number.isFinite) || w < 40 || h < 16)
                continue;
            const monIdx = monitors.findIndex(m =>
                x + w * 0.5 >= m.x && x + w * 0.5 < m.x + m.width &&
                y < m.y + m.height - 4 && y + h > m.y + 4);
            if (monIdx < 0)
                continue;
            const m = monitors[monIdx];
            // Skip the top bar (near monitor top) and slid-away autohidden docks.
            if (y <= m.y + 48 || y >= m.y + m.height - 8 || h > m.height * 0.3)
                continue;
            const rx = Math.max(m.x, Math.round(x));
            const ry = Math.max(m.y, Math.round(y));
            const rw = Math.min(m.x + m.width, Math.round(x + w)) - rx;
            const rh = Math.min(m.y + m.height, Math.round(y + h)) - ry;
            if (rw < 40 || rh < 12)
                continue;
            docks.push({
                id: this._dockId(child),
                monitor: monIdx,
                chrome: child,
                rect: {x: rx, y: ry, width: rw, height: rh},
            });
        }
        const own = new Set(this._overlays.map(o => o.window));
        for (const actor of global.get_window_actors()) {
            const w = actor.meta_window;
            if (!w || own.has(w) || w.minimized || w.is_hidden() || !actor.visible)
                continue;
            if (w.get_window_type() !== Meta.WindowType.DOCK)
                continue;
            const r = w.get_frame_rect();
            const monIdx = w.get_monitor();
            const m = monitors[monIdx];
            if (!m || r.width < 40 || r.height < 16 || r.y <= m.y + 48 || r.y >= m.y + m.height - 8)
                continue;
            docks.push({
                id: w.get_stable_sequence(),
                monitor: monIdx,
                chrome: null,
                rect: {x: r.x, y: r.y, width: r.width, height: r.height},
            });
        }
        return docks;
    }

    _snapshot() {
        const own = new Set(this._overlays.map(o => o.window));
        const workspace = global.workspace_manager.get_active_workspace();
        const windows = global.get_window_actors()
            .map(actor => actor.meta_window)
            .filter(w => !own.has(w) && !w.minimized && !w.is_hidden() &&
                w.located_on_workspace(workspace) &&
                [Meta.WindowType.NORMAL, Meta.WindowType.DIALOG, Meta.WindowType.MODAL_DIALOG]
                    .includes(w.get_window_type()))
            .map(w => ({id: w.get_stable_sequence(), rect: w.get_frame_rect()}));
        const docks = this._findDocks();
        this._docks = docks;
        const focus = global.display.focus_window;
        const [x, y, mods] = global.get_pointer();
        this._pointerDown = (mods & Clutter.ModifierType.BUTTON1_MASK) !== 0;
        return {
            windows,
            docks,
            focused: focus && !own.has(focus) ? focus.get_frame_rect() : null,
            pointer: [x, y],
            pointerDown: this._pointerDown,
            idle: global.backend.get_core_idle_monitor().get_idletime(),
            onBattery: this._onBattery,
        };
    }

    _tick() {
        const snapshot = this._snapshot();
        const nowMs = GLib.get_monotonic_time() / 1000;
        this._lastTickMs = nowMs;
        // Backgrounds are rebuilt when the wallpaper or displays change.
        this._holdBackgroundMenu(this._watchesButton());
        this._busy = this._overlays.map(o => o.tick(snapshot, nowMs)).some(Boolean);
    }
}
