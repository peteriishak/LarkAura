// Throwaway M0 spike: overlay placement plus a test-only input and state API.
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const IFACE = `<node><interface name="dev.larklabs.AuraSpike">
  <method name="LaunchOverlay"><arg type="as" direction="in" name="argv"/></method>
  <method name="StopOverlay"/>
  <method name="SetOverlayType"><arg type="s" direction="in" name="type"/></method>
  <method name="Mark"><arg type="s" direction="in" name="label"/></method>
  <method name="State"><arg type="s" direction="out" name="json"/></method>
  <method name="Pointer"><arg type="s" direction="in" name="action"/><arg type="d" direction="in" name="x"/><arg type="d" direction="in" name="y"/></method>
  <method name="Scroll"/>
  <method name="Key"/>
  <method name="Place"><arg type="s" direction="in" name="title"/><arg type="i" direction="in" name="x"/><arg type="i" direction="in" name="y"/><arg type="i" direction="in" name="w"/><arg type="i" direction="in" name="h"/></method>
  <method name="Activate"><arg type="s" direction="in" name="title"/></method>
  <method name="Fullscreen"><arg type="s" direction="in" name="title"/><arg type="b" direction="in" name="on"/></method>
  <method name="SwitchWorkspace"><arg type="i" direction="in" name="index"/></method>
  <method name="Screenshot"><arg type="s" direction="in" name="path"/></method>
</interface></node>`;

const now = () => GLib.get_monotonic_time();

// Mutter aborts if window state changes while a frame is being painted, and
// signals such as first-frame fire during paint. Always leave that stack first.
const later = fn => GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
    fn();
    return GLib.SOURCE_REMOVE;
});

export default class SpikeExtension extends Extension {
    enable() {
        const seat = Clutter.get_default_backend().get_default_seat();
        this._ptr = seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        this._kbd = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        this._signals = [
            global.display.connect('window-created', (_d, w) => this._onWindow(w)),
            global.display.connect('restacked', () => later(() => this._raise())),
            global.display.connect('notify::focus-window', () => {
                const w = global.display.focus_window;
                if (w && w === this._overlay)
                    this._overlayFocusCount = (this._overlayFocusCount ?? 0) + 1;
                this.Mark(`focus -> ${w?.get_title() ?? 'none'}`);
            }),
        ];
        // The test session must show windows, so never stay in the overview.
        this._overviewShown = Main.overview.connect('shown', () => Main.overview.hide());
        this._overviewLog = ['showing', 'shown', 'hiding', 'hidden'].map(n =>
            Main.overview.connect(n, () => this.Mark(`overview ${n}`)));
        Main.overview.hide();
        this._captured = global.stage.connect('captured-event', (_s, e) => {
            if (e.type() === Clutter.EventType.BUTTON_PRESS) {
                const a = global.stage.get_event_actor(e);
                const [x, y] = e.get_coords();
                const w = a?.meta_window ?? a?.get_parent?.()?.meta_window;
                const chain = [];
                for (let p = a; p; p = p.get_parent())
                    chain.push(`${p.constructor.name}${p.name ? `#${p.name}` : ''}[${p.width}x${p.height}${p.reactive ? ',reactive' : ''}]`);
                console.log(`[spike] ${Math.round(now() / 1000)} press at ${x},${y} window=${w?.get_title?.()} chain=${chain.join(' < ')} overview=${Main.overview.visible}/${Main.overview.animationInProgress} coverVisible=${Main.overview._coverPane?.visible} sinceOverlayMs=${this._overlayAt ? (now() - this._overlayAt) / 1000 : -1}`);
            }
            return Clutter.EVENT_PROPAGATE;
        });
        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE, this);
        this._dbus.export(Gio.DBus.session, '/dev/larklabs/AuraSpike');
        this._name = Gio.bus_own_name(Gio.BusType.SESSION, 'dev.larklabs.AuraSpike',
            Gio.BusNameOwnerFlags.NONE, null, null, null);
    }

    disable() {
        Gio.bus_unown_name(this._name);
        this._dbus.unexport();
        this._signals.forEach(id => global.display.disconnect(id));
        Main.overview.disconnect(this._overviewShown);
        this._overviewLog.forEach(id => Main.overview.disconnect(id));
        global.stage.disconnect(this._captured);
        this.StopOverlay();
        this._ptr = this._kbd = this._dbus = null;
    }

    _onWindow(win) {
        if (!this._client?.owns_window(win))
            return;
        this._overlay = win;
        this._overlayAt = now();
        this.Mark('overlay window created');
        if (this._type)
            win.set_type(Meta.WindowType[this._type]);
        win.hide_from_window_list();
        this._previousFocus = global.display.focus_window;
        const actor = win.get_compositor_private();
        const place = () => {
            // Stay in the work area: the panel and dock remain clear and readable.
            const m = Main.layoutManager.getWorkAreaForMonitor(win.get_monitor());
            win.stick();
            win.make_above();
            win.move_resize_frame(true, m.x, m.y, m.width, m.height);
            this._raise();
            if (global.display.focus_window === win && this._previousFocus)
                this._previousFocus.activate(global.get_current_time());
        };
        const id = actor.connect('first-frame', () => {
            actor.disconnect(id);
            later(place);
        });
        win.connect('focus', () => later(() => {
            if (this._overlay === win && global.display.focus_window === win && this._previousFocus)
                this._previousFocus.activate(global.get_current_time());
        }));
    }

    _raise() {
        const actor = this._overlay?.get_compositor_private();
        if (actor && global.window_group.get_last_child() !== actor)
            global.window_group.set_child_above_sibling(actor, null);
    }

    _find(title) {
        return global.get_window_actors().map(a => a.meta_window)
            .find(w => (w.get_title() ?? '').includes(title));
    }

    SetOverlayType(type) {
        this._type = type === 'NORMAL' ? null : type;
    }

    Mark(label) {
        console.log(`[spike-t] ${Math.round(now() / 1000)} ${label}`);
    }

    LaunchOverlay(argv) {
        this.StopOverlay();
        const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
        this._client = Meta.WaylandClient.new_subprocess(global.context, launcher, argv);
    }

    StopOverlay() {
        this._overlayFocusCount = 0;
        this._client?.get_subprocess()?.force_exit();
        this._client = this._overlay = this._previousFocus = null;
    }

    State() {
        const rect = w => {
            const r = w.get_frame_rect();
            return [r.x, r.y, r.width, r.height];
        };
        const [x, y] = global.get_pointer();
        const actor = global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE, x, y);
        return JSON.stringify({
            overview: Main.overview.visible,
            startingUp: Main.layoutManager._startingUp,
            focus: global.display.focus_window?.get_title() ?? null,
            pointer: [x, y],
            workArea: (a => [a.x, a.y, a.width, a.height])(Main.layoutManager.getWorkAreaForMonitor(Main.layoutManager.primaryIndex)),
            workAreas: Main.layoutManager.monitors.map((_m, i) => {
                const a = Main.layoutManager.getWorkAreaForMonitor(i);
                return [a.x, a.y, a.width, a.height];
            }),
            indicator: this._indicatorState(),
            backgroundMenuOpen: Main.layoutManager._bgManagers
                .some(m => m.backgroundActor?._backgroundMenu?.isOpen === true),
            pick: actor?.constructor?.name ?? null,
            workspace: global.workspace_manager.get_active_workspace_index(),
            tabList: global.display.get_tab_list(Meta.TabList.NORMAL, null).map(w => w.get_title()),
            overlayFocusCount: this._overlayFocusCount ?? 0,
            overlayType: this._overlay?.get_window_type() ?? null,
            overlayPid: this._client?.get_subprocess()?.get_identifier() ?? null,
            shellPid: new Gio.Credentials().get_unix_pid(),
            windows: global.get_window_actors().map(a => {
                const w = a.meta_window;
                return {title: w.get_title(), rect: rect(w), pid: w.get_pid(), type: w.get_window_type(),
                    overlay: w === this._overlay || w.get_title() === 'LarkAura Overlay',
                    above: w.is_above(), onAll: w.is_on_all_workspaces(),
                    visible: a.visible, fullscreen: w.is_fullscreen(), skip: w.is_skip_taskbar()};
            }),
        });
    }

    _indicatorState() {
        const box = a => {
            const [x, y] = a.get_transformed_position();
            const [w, h] = a.get_transformed_size();
            return [x, y, w, h];
        };
        const button = Main.panel.statusArea['larkaura@larklabs.dev'];
        if (!button)
            return null;
        return {
            rect: box(button), open: button.menu.isOpen,
            items: button.menu._getMenuItems()
                .flatMap(i => i._getMenuItems ? i._getMenuItems() : [i])
                .filter(i => i.label && i.visible).map(i => ({
                label: i.label.text, rect: box(i), ornament: i._ornament ?? null,
                toggled: i.state ?? null,
            })).concat((button._actions ?? []).filter(([b]) => b.visible).map(([b]) => ({
                // The quick actions are icon buttons, named for screen readers.
                label: b.accessible_name, rect: box(b), ornament: null, toggled: null,
            }))),
        };
    }

    Pointer(action, x, y) {
        if (action === 'move')
            this._ptr.notify_absolute_motion(now(), x, y);
        else
            this._ptr.notify_button(now(), Clutter.BUTTON_PRIMARY,
                action === 'press' ? Clutter.ButtonState.PRESSED : Clutter.ButtonState.RELEASED);
    }

    Scroll() {
        this._ptr.notify_discrete_scroll(now(), Clutter.ScrollDirection.DOWN, Clutter.ScrollSource.WHEEL);
    }

    Key() {
        this._kbd.notify_keyval(now(), Clutter.KEY_a, Clutter.KeyState.PRESSED);
        this._kbd.notify_keyval(now(), Clutter.KEY_a, Clutter.KeyState.RELEASED);
    }

    Place(title, x, y, w, h) {
        const win = this._find(title);
        win.unmaximize?.();
        win.move_resize_frame(true, x, y, w, h);
    }

    Activate(title) {
        this._find(title).activate(global.get_current_time());
    }

    Fullscreen(title, on) {
        const win = this._find(title);
        if (on)
            win.make_fullscreen();
        else
            win.unmake_fullscreen();
    }

    SwitchWorkspace(index) {
        const wm = global.workspace_manager;
        while (wm.n_workspaces <= index)
            wm.append_new_workspace(false, global.get_current_time());
        wm.get_workspace_by_index(index).activate(global.get_current_time());
    }

    ScreenshotAsync([path], invocation) {
        const stream = Gio.File.new_for_path(path).replace(null, false, Gio.FileCreateFlags.NONE, null);
        new Shell.Screenshot().screenshot(false, stream, (shot, res) => {
            try {
                shot.screenshot_finish(res);
                stream.close(null);
                invocation.return_value(null);
            } catch (e) {
                invocation.return_error_literal(Gio.DBusError, Gio.DBusError.FAILED, e.message);
            }
        });
    }
}
