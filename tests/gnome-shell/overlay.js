// Throwaway overlay client: transparent window, drifting dots, empty input region.
import cairo from 'cairo';
import Gdk from 'gi://Gdk?version=4.0';
import GLib from 'gi://GLib';
import Gtk from 'gi://Gtk?version=4.0';
import {programArgs} from 'system';

const captureInput = programArgs.includes('--capture-input');
Gtk.init();
const css = new Gtk.CssProvider();
css.load_from_string('window, window.background { background: transparent; box-shadow: none; }');
Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);

const win = new Gtk.Window({title: 'LarkAura Overlay Spike', decorated: false, default_width: 640, default_height: 360});
const area = new Gtk.DrawingArea({hexpand: true, vexpand: true});
win.set_child(area);
const dots = Array.from({length: 80}, (_, i) => ({x: (i * 97) % 1280, y: (i * 53) % 720, s: 20 + (i % 5) * 12}));
let t = 0;
area.set_draw_func((_a, cr, w, h) => {
    cr.setSourceRGBA(0.93, 0.42, 0.17, 0.9);
    for (const d of dots) {
        cr.arc(d.x % w, (d.y + t * d.s) % h, 5, 0, 2 * Math.PI);
        cr.fill();
    }
    cr.$dispose();
});
const applyInput = () => {
    if (!captureInput)
        win.get_surface()?.set_input_region(new cairo.Region());
};
win.connect('realize', applyInput);
win.connect('notify::default-width', applyInput);
area.connect('resize', applyInput);
GLib.timeout_add(GLib.PRIORITY_DEFAULT, 33, () => {
    t += 0.033;
    applyInput();              // GTK can replace the region after it resizes the surface
    area.queue_draw();
    return GLib.SOURCE_CONTINUE;
});
// Diagnostic: with an empty input region this must never fire.
const raw = new Gtk.EventControllerLegacy();
raw.connect('event', (_c, e) => {
    const t = e.get_event_type();
    if (t !== Gdk.EventType.TOUCHPAD_HOLD)
        print(`[overlay-event] ${Math.round(GLib.get_monotonic_time() / 1000)} type=${t} region=${captureInput ? 'full' : 'empty'}`);
    return false;
});
win.add_controller(raw);
win.present();
new GLib.MainLoop(null, false).run();
