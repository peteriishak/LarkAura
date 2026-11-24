// Throwaway target: records every input event it actually receives.
import Gdk from 'gi://Gdk?version=4.0';
import GLib from 'gi://GLib';
import Gtk from 'gi://Gtk?version=4.0';
import {programArgs} from 'system';

const title = programArgs[0] ?? 'Target';
Gtk.init();
const log = e => print(JSON.stringify({title, ms: Math.round(GLib.get_monotonic_time() / 1000), ...e}));
const win = new Gtk.Window({title, default_width: 700, default_height: 420});
const box = new Gtk.Label({label: title, hexpand: true, vexpand: true});
win.set_child(box);
const click = new Gtk.GestureClick({button: Gdk.BUTTON_PRIMARY});
click.connect('pressed', () => log({e: 'press'}));
click.connect('released', () => log({e: 'release'}));
win.add_controller(click);
const motion = new Gtk.EventControllerMotion();
motion.connect('enter', () => log({e: 'enter'}));
motion.connect('motion', () => log({e: 'motion'}));
win.add_controller(motion);
const scroll = new Gtk.EventControllerScroll({flags: Gtk.EventControllerScrollFlags.VERTICAL});
scroll.connect('scroll', () => {
    log({e: 'scroll'});
    return true;
});
win.add_controller(scroll);
const drag = new Gtk.GestureDrag({button: Gdk.BUTTON_PRIMARY});
drag.connect('drag-end', (_g, x, y) => log({e: 'drag', x, y}));
win.add_controller(drag);
const key = new Gtk.EventControllerKey();
key.connect('key-pressed', (_c, keyval) => {
    log({e: 'key', keyval});
    return false;
});
win.add_controller(key);
const raw = new Gtk.EventControllerLegacy();
raw.connect('event', (_c, ev) => {
    log({e: 'raw', type: ev.get_event_type()});
    return false;
});
win.add_controller(raw);
win.present();
new GLib.MainLoop(null, false).run();
