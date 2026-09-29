"""In-process eSpeak synthesis and PortAudio playback: no shell or audio files."""
import ctypes
import ctypes.util
import os


def synthesize(text, voice=''):
    library=os.environ.get('CLICKY_ESPEAK_LIBRARY') or ctypes.util.find_library('espeak-ng') or ctypes.util.find_library('espeak')
    if not library: raise RuntimeError('Install eSpeak NG for local Linux speech output.')
    engine=ctypes.CDLL(library)
    engine.espeak_Initialize.argtypes=[ctypes.c_int,ctypes.c_int,ctypes.c_char_p,ctypes.c_int]
    engine.espeak_Initialize.restype=ctypes.c_int
    engine.espeak_SetVoiceByName.argtypes=[ctypes.c_char_p]
    engine.espeak_SetParameter.argtypes=[ctypes.c_int,ctypes.c_int,ctypes.c_int]
    engine.espeak_Synth.argtypes=[ctypes.c_void_p,ctypes.c_size_t,ctypes.c_uint,ctypes.c_int,ctypes.c_uint,ctypes.c_uint,ctypes.POINTER(ctypes.c_uint),ctypes.c_void_p]
    engine.espeak_Synth.restype=ctypes.c_int
    callback_type=ctypes.CFUNCTYPE(ctypes.c_int,ctypes.POINTER(ctypes.c_short),ctypes.c_int,ctypes.c_void_p)
    engine.espeak_SetSynthCallback.argtypes=[callback_type]
    # Optional path is useful for portable distributions, otherwise native engine defaults.
    path=os.environ.get('CLICKY_ESPEAK_DATA')
    rate=engine.espeak_Initialize(2,0,path.encode() if path else None,0)
    if rate<=0: raise RuntimeError('eSpeak initialization failed.')
    pcm=bytearray()
    too_large=False
    @callback_type
    def receive(samples,count,events):
        nonlocal too_large
        if count>0 and bool(samples):
            if len(pcm)+count*2>4*1024*1024: too_large=True;return 1
            pcm.extend(ctypes.string_at(samples,count*2))
        return 0
    try:
        engine.espeak_SetSynthCallback(receive)
        if voice and engine.espeak_SetVoiceByName(voice.encode())!=0: raise RuntimeError('Unknown local voice identifier.')
        engine.espeak_SetParameter(1,175,0)
        data=text.encode('utf-8')+b'\0'
        if engine.espeak_Synth(data,len(data),0,1,0,1,None,None)!=0 or too_large: raise RuntimeError('Speech synthesis failed or exceeded its size limit.')
        if not pcm: raise RuntimeError('Speech synthesis produced no audio.')
        return bytes(pcm),rate
    finally: engine.espeak_Terminate()


def speak(text,voice=''):
    import sounddevice as sd
    pcm,rate=synthesize(text,voice)
    with sd.RawOutputStream(samplerate=rate,channels=1,dtype='int16') as output:
        # Modest chunks limit queued audio when the worker is cancelled.
        for offset in range(0,len(pcm),4096): output.write(pcm[offset:offset+4096])
