"""One activated voice operation per process. Never starts microphone on import/probe.
Protocol: one JSON request on stdin, bounded JSON-line events on stdout.
"""
import array
import io
import json
import math
import os
from pathlib import Path
import queue
import sys
import time
import urllib.request
import urllib.parse
import wave

MAX_SECONDS = 20
SAMPLE_RATE = 16000

class VoiceError(Exception):
    def __init__(self, code, message):
        self.code, self.message = code, message
        super().__init__(message)

def emit(event, **values):
    print(json.dumps({"event": event, **values}), flush=True)

class VoiceActivity:
    """Bounded energy VAD; model recognition follows after capture closes."""
    def __init__(self, threshold=350):
        self.threshold = threshold
        self.started = False
        self.voiced = 0
        self.silence = 0
    def feed(self, pcm):
        samples = array.array('h', pcm)
        if sys.byteorder != 'little': samples.byteswap()
        duration = len(samples) / SAMPLE_RATE
        energy = math.sqrt(sum(x*x for x in samples) / max(1, len(samples)))
        if energy >= self.threshold:
            self.voiced += duration
            self.started = self.voiced >= .15
            self.silence = 0
        elif self.started:
            self.silence += duration
        return self.started and self.silence >= 1.1

def capture(device=None):
    import sounddevice as sd
    chunks = queue.Queue(maxsize=64)
    def callback(indata, frames, timing, status):
        try: chunks.put_nowait((bytes(indata), bool(status)))
        except queue.Full: pass
    activity = VoiceActivity()
    audio = bytearray()
    # The stream is closed before transcription/network work starts.
    try:
        with sd.RawInputStream(samplerate=SAMPLE_RATE, channels=1, dtype='int16',
                               blocksize=1600, device=device, callback=callback):
            emit('state', state='LISTENING')
            deadline=time.monotonic()+MAX_SECONDS
            while time.monotonic()<deadline and len(audio)<SAMPLE_RATE*2*MAX_SECONDS:
                try: pcm, overflow=chunks.get(timeout=2)
                except queue.Empty: raise VoiceError('MICROPHONE_TIMEOUT','The microphone did not deliver audio.')
                if overflow: raise VoiceError('AUDIO_OVERFLOW','Audio capture was interrupted. Please try again.')
                audio.extend(pcm)
                if activity.feed(pcm): break
                if not activity.started and len(audio)>=SAMPLE_RATE*2*8:
                    raise VoiceError('NO_SPEECH','No speech heard. Press Alt+X to try again.')
    except sd.PortAudioError as exc:
        raise VoiceError('MICROPHONE_UNAVAILABLE','Microphone unavailable or permission denied. Check the input device in Settings.') from exc
    if not activity.started: raise VoiceError('NO_SPEECH','No speech heard. Press Alt+X to try again.')
    return bytes(audio)

class SpeechProvider:
    def transcribe(self, pcm): raise NotImplementedError

class LocalSpeechProvider(SpeechProvider):
    def __init__(self, config):
        from vosk import Model, SetLogLevel
        model=config.get('model_path') or os.environ.get('CLICKY_STT_MODEL','')
        if not model or not Path(model).is_dir():
            raise VoiceError('MODEL_MISSING','Install an offline Vosk model and select its folder in Voice settings.')
        SetLogLevel(-1)
        self.model=Model(model)
    def transcribe(self, pcm):
        from vosk import KaldiRecognizer
        recognizer=KaldiRecognizer(self.model, SAMPLE_RATE)
        recognizer.AcceptWaveform(pcm)
        return json.loads(recognizer.FinalResult()).get('text','')

def cloud_request(config, payload, content_type, max_bytes):
    endpoint=config.get('endpoint','')
    url=urllib.parse.urlsplit(endpoint)
    if not config.get('cloud_audio') or url.scheme!='https' or not url.netloc or url.username or url.password:
        raise VoiceError('CLOUD_DISABLED','Cloud audio requires explicit consent and an HTTPS endpoint.')
    token=os.environ.get('CLICKY_SPEECH_API_KEY','')
    if not token: raise VoiceError('AUTH_MISSING','Set CLICKY_SPEECH_API_KEY before using cloud voice.')
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self,*args,**kwargs): return None
    request=urllib.request.Request(endpoint,data=payload,headers={'Content-Type':content_type,'Authorization':'Bearer '+token})
    with urllib.request.build_opener(NoRedirect).open(request,timeout=15) as response:
        data=response.read(max_bytes+1)
    if len(data)>max_bytes: raise VoiceError('OUTPUT_TOO_LARGE','Voice provider response exceeded its limit.')
    return data

class CloudSpeechProvider(SpeechProvider):
    def __init__(self, config):
        self.config=config
        if not config.get('cloud_audio'):
            raise VoiceError('CLOUD_DISABLED','Enable cloud audio explicitly in Voice settings.')
        if not config.get('model'): raise VoiceError('MODEL_MISSING','Choose a speech recognition model.')
    def transcribe(self, pcm):
        wav=io.BytesIO()
        with wave.open(wav,'wb') as output:
            output.setnchannels(1);output.setsampwidth(2);output.setframerate(SAMPLE_RATE);output.writeframes(pcm)
        import uuid
        boundary=uuid.uuid4().hex
        model=self.config['model']
        if len(model)>200 or '\r' in model or '\n' in model: raise VoiceError('INVALID_CONFIG','Invalid model name.')
        body=(f'--{boundary}\r\nContent-Disposition: form-data; name="model"\r\n\r\n{model}\r\n'
              f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="speech.wav"\r\nContent-Type: audio/wav\r\n\r\n').encode()+wav.getvalue()+f'\r\n--{boundary}--\r\n'.encode()
        return json.loads(cloud_request(self.config,body,'multipart/form-data; boundary='+boundary,16384)).get('text','')

class TTSProvider:
    def speak(self, text): raise NotImplementedError

class LocalTTS(TTSProvider):
    def __init__(self, config): self.config=config
    def speak(self, text):
        if sys.platform.startswith("linux"):
            from local_tts import speak
            try: speak(text,self.config.get("voice", ""))
            except Exception as exc: raise VoiceError("TTS_UNAVAILABLE","Local speech output needs eSpeak NG and an available audio output device.") from exc
            return
        import pyttsx3
        try: engine=pyttsx3.init()
        except Exception as exc: raise VoiceError('TTS_UNAVAILABLE','Local speech output requires Windows SAPI or Linux eSpeak NG.') from exc
        voice=self.config.get('voice','')
        if voice: engine.setProperty('voice',voice)
        engine.setProperty('rate',175)
        try: engine.say(text);engine.runAndWait()
        finally: engine.stop()

class CloudTTS(TTSProvider):
    def __init__(self, config): self.config=config
    def speak(self, text):
        import sounddevice as sd
        payload=json.dumps({'model':self.config.get('model'),'voice':self.config.get('voice'),'input':text,'response_format':'wav'}).encode()
        data=cloud_request(self.config,payload,'application/json',4*1024*1024)
        with wave.open(io.BytesIO(data),'rb') as audio:
            if audio.getsampwidth()!=2 or audio.getnchannels() not in (1,2):
                raise VoiceError('INVALID_AUDIO','The voice provider must return 16-bit PCM WAV.')
            with sd.RawOutputStream(samplerate=audio.getframerate(),channels=audio.getnchannels(),dtype='int16') as output:
                while True:
                    chunk=audio.readframes(2048)
                    if not chunk: break
                    output.write(chunk)

def probe():
    import importlib.util
    result={name:bool(importlib.util.find_spec(name)) for name in ('sounddevice','vosk','pyttsx3','dbus_next')}
    result['model_configured']=bool(os.environ.get('CLICKY_STT_MODEL'))
    # Device enumeration opens no capture stream.
    try:
        import sounddevice as sd
        result['microphones']=[{'id':i,'name':d['name']} for i,d in enumerate(sd.query_devices()) if d['max_input_channels']>0]
    except Exception: result['microphones']=[]
    return result

def main():
    try:
        line=sys.stdin.buffer.readline(16385)
        if len(line)>16384: raise VoiceError('INPUT_TOO_LARGE','Voice request too large.')
        request=json.loads(line)
        operation=request.get('operation')
        config=request.get('config',{})
        if operation=='portal':
            import asyncio
            from portal import run
            asyncio.run(run(emit,request.get('allow_prompt',False)));return
        if operation=='probe': emit('result',diagnostics=probe());return
        if operation=='listen':
            provider=LocalSpeechProvider(config) if config.get('provider','local')=='local' else CloudSpeechProvider(config)
            pcm=capture(config.get('microphone'))
            emit('state',state='TRANSCRIBING')
            text=provider.transcribe(pcm)
            if not isinstance(text,str) or not text.strip(): raise VoiceError('NO_TRANSCRIPT','I could not understand that. Press Alt+X to try again.')
            if len(text)>2000: raise VoiceError('INPUT_TOO_LARGE','Please use a shorter request.')
            emit('result',text=text)
        elif operation=='speak':
            text=request.get('text','')
            if not isinstance(text,str) or not text.strip() or len(text)>500: raise VoiceError('INVALID_TEXT','Spoken instructions must be short.')
            provider=LocalTTS(config) if config.get('provider','local')=='local' else CloudTTS(config)
            provider.speak(text);emit('result')
        else: raise VoiceError('INVALID_OPERATION','Unknown voice operation.')
    except VoiceError as exc: emit('error',code=exc.code,message=exc.message)
    except ImportError: emit('error',code='DEPENDENCY_MISSING',message='Install voice/requirements.txt into CLICKY_PYTHON before using voice.')
    except Exception: emit('error',code='VOICE_FAILED',message='Voice service failed. Check the selected device, model and provider configuration.')

if __name__=='__main__': main()
