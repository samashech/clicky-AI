"""Wayland global shortcut portal. No microphone, screenshots or polling."""
import asyncio
import uuid
from dbus_next import Message,MessageType,Variant
from dbus_next.aio import MessageBus

async def run(emit, allow_prompt=False):
    bus=await MessageBus().connect()
    pending={}
    early={}
    session=None
    def handler(message):
        if message.message_type==MessageType.SIGNAL and message.interface=='org.freedesktop.portal.Request' and message.member=='Response':
            if message.path in pending:
                if not pending[message.path].done(): pending[message.path].set_result(message.body)
            elif len(early)<8: early[message.path]=message.body
    bus.add_message_handler(handler)
    await bus.call(Message(destination='org.freedesktop.DBus',path='/org/freedesktop/DBus',interface='org.freedesktop.DBus',member='AddMatch',signature='s',body=["type='signal',sender='org.freedesktop.portal.Desktop',interface='org.freedesktop.portal.Request',member='Response'"]))
    async def response(path):
        if path in early: result=early.pop(path)
        else:
            pending[path]=asyncio.get_running_loop().create_future()
            try: result=await asyncio.wait_for(pending[path],120 if allow_prompt else 8)
            finally: pending.pop(path,None)
        code,values=result
        if code!=0: raise RuntimeError('Shortcut permission was declined or unavailable.')
        return values
    def token(): return 'clicky_'+uuid.uuid4().hex
    try:
        introspection=await bus.introspect('org.freedesktop.portal.Desktop','/org/freedesktop/portal/desktop')
        proxy=bus.get_proxy_object('org.freedesktop.portal.Desktop','/org/freedesktop/portal/desktop',introspection)
        interface=proxy.get_interface('org.freedesktop.portal.GlobalShortcuts')
        created=await response(await interface.call_create_session({'handle_token':Variant('s',token()),'session_handle_token':Variant('s',token())}))
        session=created['session_handle'].value
        def activated(handle,shortcut,timestamp,options):
            if handle==session and shortcut=='activate': emit('activate')
        interface.on_activated(activated)
        listed=await response(await interface.call_list_shortcuts(session,{'handle_token':Variant('s',token())}))
        shortcuts=listed.get('shortcuts',Variant('a(sa{sv})',[])).value
        if not any(item[0]=='activate' for item in shortcuts):
            if not allow_prompt:
                emit('unavailable',message='Enable the Wayland activation shortcut in Settings. The desktop may ask for permission.');return
            bound=await response(await interface.call_bind_shortcuts(session,[['activate',{'description':Variant('s','Activate or cancel ClickyAI'),'preferred_trigger':Variant('s','ALT+x')}]],'',{'handle_token':Variant('s',token())}))
            shortcuts=bound.get('shortcuts',Variant('a(sa{sv})',[])).value
        if not any(item[0]=='activate' for item in shortcuts): raise RuntimeError('The desktop did not bind the activation shortcut.')
        description=next(item[1].get('trigger_description',Variant('s','Desktop-managed shortcut')).value for item in shortcuts if item[0]=='activate')
        emit('ready',shortcut=description)
        await bus.wait_for_disconnect()
    finally:
        if session:
            try: await bus.call(Message(destination='org.freedesktop.portal.Desktop',path=session,interface='org.freedesktop.portal.Session',member='Close'))
            except Exception: pass
        bus.disconnect()
