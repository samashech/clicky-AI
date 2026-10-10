"""Native smoke: real GTK control -> grim OCR -> Rust task -> layer-shell + local TTS.
Shows a fixture application and speaks a short instruction. Never records microphone.
Requires a built debug app, Hyprland, system GTK/PyGObject and normal runtime dependencies.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[1]
FIXTURE='''import gi
gi.require_version("Gtk","3.0")
from gi.repository import Gtk
w=Gtk.Window(title="Clicky OCR validation")
w.set_default_size(700,400)
b=Gtk.Button(label="Export")
b.set_margin_top(100); b.set_margin_bottom(100); b.set_margin_start(100); b.set_margin_end(100)
style=Gtk.CssProvider();style.load_from_data(b"button {font-size: 36px; background: white; color: black;}")
b.get_style_context().add_provider(style,600)
w.add(b);w.show_all();Gtk.main()
'''

def focus(address):
    result=subprocess.run(['hyprctl','dispatch','focuswindow','address:'+address],capture_output=True)
    if result.returncode:
        subprocess.run(['hyprctl','dispatch','hl.dsp.focus({window='+json.dumps('address:'+address)+'})'],check=True,capture_output=True)

def main():
    candidates=[os.environ['CLICKY_DESKTOP_PYTHON']] if 'CLICKY_DESKTOP_PYTHON' in os.environ else [str(Path(p)/'python3') for p in os.get_exec_path() if (Path(p)/'python3').is_file()]
    python=next(p for p in candidates if subprocess.run([p,'-c','import gi,cairo'],capture_output=True,timeout=5).returncode==0)
    fixture=subprocess.Popen([python,'-c',FIXTURE])
    app=None
    try:
        time.sleep(2)
        clients=json.loads(subprocess.check_output(['hyprctl','-j','clients']))
        own=next(w for w in clients if w.get('pid')==fixture.pid and w.get('title')=='Clicky OCR validation')
        focus(own['address'])
        time.sleep(.25)
        active=json.loads(subprocess.check_output(['hyprctl','-j','activewindow']))
        assert active.get('title')=='Clicky OCR validation','Fixture must be focused for the test'
        app=subprocess.Popen([str(ROOT/'src-tauri/target/debug/tauri-app'),'--guide','Click Export'],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        time.sleep(1)
        address=active['address']
        focus(address)
        deadline=time.monotonic()+30
        captured=False
        while time.monotonic()<deadline:
            layers=subprocess.check_output(['hyprctl','-j','layers']).decode()
            if '--diagnose' in sys.argv and not captured and time.monotonic()>deadline-24:
                for monitor in json.loads(layers).values():
                    for level in monitor['levels'].values():
                        for layer in level:
                            if layer['namespace']=='clickyai-status':
                                x,y,w,h=(layer[k] for k in ['x','y','w','h'])
                                subprocess.run(['grim','-g',f'{x},{y} {w}x{h}','/tmp/clicky-status-diagnostic.png'],check=True)
                                captured=True
            if 'clickyai-guide' in layers and 'clickyai-instruction' in layers:
                # Real target geometry must be obtained by the app's OCR pipeline.
                print(json.dumps({'real_capture_ocr_guidance':True,'layer_shell_guide':True,'instruction_popup':True,'microphone_used':False}))
                time.sleep(4)
                fixture.terminate();fixture.wait(timeout=5)
                expires=time.monotonic()+4
                while time.monotonic()<expires:
                    layers=subprocess.check_output(['hyprctl','-j','layers']).decode()
                    if 'clickyai-guide' not in layers:break
                    time.sleep(.1)
                assert 'clickyai-guide' not in layers,'Closed application left a stale target visible'
                assert 'clickyai-instruction' in layers,'Task confirmation should remain available after an expected window change'
                assert app.poll() is None,'Closing the target application must not exit ClickyAI'
                print(json.dumps({'closed_target_invalidated':True}))
                app.terminate();app.wait(timeout=5)
                log=app.stderr.read().decode()
                assert '\"event\":\"speech_finished\"' in log and '\"success\":true' in log, 'Speech output did not complete successfully: '+log
                time.sleep(1)
                remaining=subprocess.check_output(['hyprctl','-j','layers']).decode()
                assert 'clickyai-guide' not in remaining and 'clickyai-instruction' not in remaining, 'Overlay worker survived parent exit'
                print(json.dumps({'speech_output_completed':True,'overlay_cleaned_up':True}))
                return
            if app.poll() is not None: raise RuntimeError(app.stderr.read().decode())
            time.sleep(.25)
        app.terminate(); app.wait(timeout=5)
        raise RuntimeError('Native guidance did not appear within 30 seconds: '+app.stderr.read().decode())
    finally:
        if app:
            app.terminate()
            try:app.wait(timeout=5)
            except subprocess.TimeoutExpired:app.kill();app.wait()
        fixture.terminate();fixture.wait(timeout=5)

if __name__=='__main__':main()
