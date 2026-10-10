"""Native, click-through Hyprland guidance. No capture, model or microphone access.
The small separate action panel accepts explicit user confirmation, never fake clicks.
"""
import json
import math
import os
import socket
import sys
import threading
from app_context import same_window
import gi
gi.require_version('Gtk', '3.0')
gi.require_version('GtkLayerShell', '0.1')
from gi.repository import Gtk, Gdk, GLib, GtkLayerShell
import cairo


def emit(event):
    print(json.dumps({'event': event}), flush=True)


def query(command):
    base = os.environ.get('XDG_RUNTIME_DIR', '')
    signature = os.environ.get('HYPRLAND_INSTANCE_SIGNATURE', '')
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(.15)
        connection.connect(os.path.join(base, 'hypr', signature, '.socket.sock'))
        connection.sendall(('j/'+command).encode())
        data=bytearray()
        while len(data)<=65536:
            part=connection.recv(4096)
            if not part: return json.loads(data)
            data.extend(part)
        raise ValueError('IPC response too large')


def main():
    request = json.loads(sys.stdin.buffer.readline(16385))
    bounds, monitor = request['bounds'], request['monitor']
    expected = request['window']
    if not same_window(expected,query('activewindow')):
        emit('stale');return
    display = Gdk.Display.get_default()
    selected = next((display.get_monitor(i) for i in range(display.get_n_monitors())
                     if display.get_monitor(i).get_geometry().x == monitor['x']
                     and display.get_monitor(i).get_geometry().y == monitor['y']), None)
    if selected is None:
        raise RuntimeError('Target monitor is unavailable')
    if not GtkLayerShell.is_supported():
        raise RuntimeError('Compositor does not support layer shell')
    def layer(namespace):
        window = Gtk.Window()
        window.set_app_paintable(True)
        window.set_visual(window.get_screen().get_rgba_visual())
        GtkLayerShell.init_for_window(window)
        GtkLayerShell.set_namespace(window, namespace)
        GtkLayerShell.set_monitor(window, selected)
        GtkLayerShell.set_layer(window, GtkLayerShell.Layer.OVERLAY)
        GtkLayerShell.set_keyboard_mode(window, GtkLayerShell.KeyboardMode.NONE)
        GtkLayerShell.set_exclusive_zone(window, -1)
        return window
    overlay = layer('clickyai-guide')
    for edge in (GtkLayerShell.Edge.TOP, GtkLayerShell.Edge.BOTTOM, GtkLayerShell.Edge.LEFT, GtkLayerShell.Edge.RIGHT):
        GtkLayerShell.set_anchor(overlay, edge, True)
    area = Gtk.DrawingArea()
    overlay.add(area)
    point = [None]
    x, y = bounds['x']-monitor['x'], bounds['y']-monitor['y']
    w, h = bounds['width'], bounds['height']
    def draw(widget, context):
        context.set_operator(cairo.OPERATOR_SOURCE)
        context.set_source_rgba(0, 0, 0, .16)
        context.paint()
        context.set_operator(cairo.OPERATOR_CLEAR)
        context.rectangle(x-6, y-6, w+12, h+12)
        context.fill()
        context.set_operator(cairo.OPERATOR_OVER)
        context.set_source_rgba(.2, .85, 1, .95)
        context.set_line_width(3)
        context.rectangle(x-7, y-7, w+14, h+14)
        context.stroke()
        if point[0]:
            px, py = point[0]['x']-monitor['x'], point[0]['y']-monitor['y']
            tx, ty = x+w/2, y+h/2
            context.move_to(px, py)
            context.curve_to(px, ty, (px+tx)/2, ty, tx, ty)
            context.stroke()
            context.arc(tx, ty, 5, 0, math.tau)
            context.fill()
        return False
    area.connect('draw', draw)
    overlay.connect('realize', lambda window: window.get_window().input_shape_combine_region(cairo.Region(), 0, 0))
    panel = layer('clickyai-instruction')
    GtkLayerShell.set_anchor(panel, GtkLayerShell.Edge.BOTTOM, True)
    GtkLayerShell.set_margin(panel, GtkLayerShell.Edge.BOTTOM, 32)
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
    box.set_border_width(16)
    label = Gtk.Label(label=request['instruction'])
    label.set_line_wrap(True)
    label.set_max_width_chars(56)
    label.set_width_chars(48)
    box.pack_start(label, False, False, 0)
    note = Gtk.Label(label='Do this in your application, then confirm to continue.')
    note.set_line_wrap(True)
    note.set_max_width_chars(56)
    note.set_width_chars(48)
    box.pack_start(note, False, False, 0)
    buttons = Gtk.Box(spacing=12)
    finished = [False]
    invalidated = [False]
    def invalidate():
        if finished[0] or invalidated[0]: return False
        invalidated[0] = True
        overlay.hide()
        note.set_text('The screen changed. Confirm if you completed the action, or choose Find again.')
        emit('stale')
        return False
    def act(event):
        if finished[0]: return False
        if event == 'confirmed' and not invalidated[0]:
            try:
                if not same_window(expected, query('activewindow')): return invalidate()
            except (OSError, ValueError): return invalidate()
        finished[0] = True
        overlay.hide()
        panel.hide()
        emit(event)
        Gtk.main_quit()
        return False
    for title, event in [('I did this — next step', 'confirmed'), ('Find again', 'recheck'), ('Cancel', 'cancel')]:
        button = Gtk.Button(label=title)
        button.connect('clicked', lambda widget, event=event: act(event))
        buttons.pack_start(button, True, True, 0)
    box.pack_start(buttons, False, False, 0)
    panel.add(box)
    overlay.show_all()
    panel.show_all()
    emit('ready')
    def update(value):
        if finished[0] or invalidated[0] or value == point[0]: return False
        point[0] = value
        area.queue_draw()
        return False
    parent = os.getppid()
    def poll():
        # Runs only while this guidance process exists, never while Clicky is idle.
        import time
        next_check=0
        while os.getppid() == parent and not finished[0]:
            if invalidated[0]:
                time.sleep(.25);continue
            try:
                if time.monotonic()>=next_check:
                    if not same_window(expected,query('activewindow')):
                        GLib.idle_add(invalidate)
                        time.sleep(.25);continue
                    next_check=time.monotonic()+.25
                GLib.idle_add(update, query('cursorpos'))
            except (OSError, ValueError):
                GLib.idle_add(invalidate)
                time.sleep(.25);continue
            time.sleep(1/30)
        GLib.idle_add(Gtk.main_quit)
    threading.Thread(target=poll, daemon=True).start()
    Gtk.main()

if __name__ == '__main__':
    try: main()
    except Exception:
        emit('error')
        sys.exit(1)
