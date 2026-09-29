import array
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('voice_worker',ROOT/'voice/worker.py')
voice=importlib.util.module_from_spec(spec);spec.loader.exec_module(voice)

class VoiceTests(unittest.TestCase):
    def test_silence_does_not_start_a_session(self):
        vad=voice.VoiceActivity()
        for _ in range(90): self.assertFalse(vad.feed(bytes(3200)))
        self.assertFalse(vad.started)
    def test_vad_ends_after_speech_and_silence(self):
        vad=voice.VoiceActivity()
        loud=array.array('h',[1200]*1600).tobytes()
        for _ in range(3): self.assertFalse(vad.feed(loud))
        self.assertTrue(vad.started)
        ended=False
        for _ in range(12): ended=vad.feed(bytes(3200))
        self.assertTrue(ended)
    def test_cloud_audio_requires_consent_before_network(self):
        with patch('urllib.request.build_opener',side_effect=AssertionError('Network accessed')):
            with self.assertRaises(voice.VoiceError): voice.cloud_request({'endpoint':'https://example.invalid'},b'private','audio/wav',100)
    def test_unknown_operation_has_no_microphone(self):
        result=subprocess.run([sys.executable,str(ROOT/'voice/worker.py')],input=b'{"operation":"unknown"}\n',capture_output=True,timeout=5,check=True)
        error=json.loads(result.stdout)
        self.assertEqual(error['event'],'error');self.assertEqual(error['code'],'INVALID_OPERATION')
    def test_oversized_request_fails_before_capture(self):
        result=subprocess.run([sys.executable,str(ROOT/'voice/worker.py')],input=b'x'*16385,capture_output=True,timeout=5,check=True)
        self.assertEqual(json.loads(result.stdout)['code'],'INPUT_TOO_LARGE')
    def test_local_model_path_required(self):
        # Dependency-free test substitutes only model loading; no invented transcription.
        import types
        module=types.SimpleNamespace(Model=lambda path: self.fail('model should not load'),SetLogLevel=lambda level:None)
        with patch.dict(sys.modules,{'vosk':module}):
            with self.assertRaises(voice.VoiceError) as error: voice.LocalSpeechProvider({'model_path':str(ROOT/'missing-model')})
        self.assertEqual(error.exception.code,'MODEL_MISSING')
    def test_stereo_cloud_or_unknown_pcm_rejected_without_playback(self):
        # Format parsing is bounded by wave; malformed provider data must fail safely.
        import types
        with patch.dict(sys.modules,{'sounddevice':types.SimpleNamespace()}),patch.object(voice,'cloud_request',return_value=b'invalid'):
            with self.assertRaises(Exception): voice.CloudTTS({}).speak('Click File')

if __name__=='__main__':unittest.main()
