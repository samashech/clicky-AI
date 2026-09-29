"""Small non-focus-stealing session indicator on a Wayland layer surface."""
import json
import os
import sys
import gi
gi.require_version('Gtk','3.0')
gi.require_version('GtkLayerShell','0.1')
from gi.repository import Gtk, GLib, GtkLayerShell

request=json.loads(sys.stdin.buffer.readline(16385))
window=Gtk.Window()
GtkLayerShell.init_for_window(window)
GtkLayerShell.set_namespace(window,'clickyai-status')
GtkLayerShell.set_layer(window,GtkLayerShell.Layer.OVERLAY)
GtkLayerShell.set_keyboard_mode(window,GtkLayerShell.KeyboardMode.NONE)
GtkLayerShell.set_exclusive_zone(window,-1)
for edge in (GtkLayerShell.Edge.TOP,GtkLayerShell.Edge.RIGHT):
    GtkLayerShell.set_anchor(window,edge,True)
    GtkLayerShell.set_margin(window,edge,24)
box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=8)
box.set_border_width(12)
label=Gtk.Label(label=request['message'])
label.set_line_wrap(True)
label.set_max_width_chars(42)
label.set_width_chars(38)
box.pack_start(label,False,False,0)
button=Gtk.Button(label='Cancel')
button.connect('clicked',lambda w: print(json.dumps({'event':'cancel'}),flush=True))
box.pack_start(button,False,False,0)
window.add(box)
window.show_all()
parent=os.getppid()
def check_parent():
    if os.getppid()!=parent:Gtk.main_quit();return False
    return True
GLib.timeout_add(500,check_parent)
Gtk.main()
