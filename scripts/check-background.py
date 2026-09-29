"""Native Hyprland smoke test. Requires ClickyAI already running and dbus-next.
Checks hidden startup + tray, explicitly opens Settings, then closes Settings.
Never activates the microphone. Only addresses ClickyAI's tray and window.
"""
import asyncio
import json
import subprocess
import sys
from dbus_next import Variant
from dbus_next.aio import MessageBus

async def main():
    bus=await MessageBus().connect()
    async def proxy(service,path):
        return bus.get_proxy_object(service,path,await bus.introspect(service,path))
    def windows():
        return [w for w in json.loads(subprocess.check_output(['hyprctl','-j','clients'])) if w.get('title','').startswith('ClickyAI')]
    try:
        assert not windows(), 'ClickyAI should have no visible window at idle'
        watcher=await proxy('org.kde.StatusNotifierWatcher','/StatusNotifierWatcher')
        items=(await watcher.get_interface('org.freedesktop.DBus.Properties').call_get('org.kde.StatusNotifierWatcher','RegisteredStatusNotifierItems')).value
        item=next(item for item in items if 'tray_app_clicky' in item)
        service,tail=item.split('/',1);path='/'+tail
        item_proxy=await proxy(service,path)
        menu_path=(await item_proxy.get_interface('org.freedesktop.DBus.Properties').call_get('org.kde.StatusNotifierItem','Menu')).value
        menu=(await proxy(service,menu_path)).get_interface('com.canonical.dbusmenu')
        _,layout=await menu.call_get_layout(0,-1,[])
        def entries(node):
            yield node[0],node[1].get('label',Variant('s','')).value
            for child in node[2]: yield from entries(child.value)
        choices=dict((label,identifier) for identifier,label in entries(layout))
        for required in ['Activate Clicky','Pause / Resume','Settings','Diagnostics','Quit']: assert required in choices,choices
        await menu.call_event(choices['Settings'],'clicked',Variant('i',0),0)
        await asyncio.sleep(1)
        settings=next(w for w in windows() if w['title']=='ClickyAI Settings')
        close = subprocess.run(['hyprctl','dispatch','closewindow','address:'+settings['address']],capture_output=True)
        if close.returncode:
            # Current Hyprland uses Lua dispatchers; older releases use the above syntax.
            selector = json.dumps('address:' + settings['address'])
            subprocess.run(['hyprctl','dispatch',f'hl.dsp.window.close({{window={selector}}})'],check=True,capture_output=True)
        await asyncio.sleep(.5)
        assert not windows(),'Closing Settings should return to background'
        if '--quit' in sys.argv:
            await menu.call_event(choices['Quit'],'clicked',Variant('i',0),0)
            await asyncio.sleep(1)
            remaining=(await watcher.get_interface('org.freedesktop.DBus.Properties').call_get('org.kde.StatusNotifierWatcher','RegisteredStatusNotifierItems')).value
            assert item not in remaining, 'Quit should unregister the tray'
        print(json.dumps({'startup_windows':0,'tray_menu':list(choices),'settings_opened':True,'settings_closed_to_background':True,'microphone_activated':False}))
    finally: bus.disconnect()

if __name__=='__main__':asyncio.run(main())
