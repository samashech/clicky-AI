import base64
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('perception', ROOT/'omni_server/main.py')
perception=importlib.util.module_from_spec(spec)
spec.loader.exec_module(perception)

class PerceptionTest(unittest.TestCase):
    def test_invalid_image_rejected(self):
        with self.assertRaises(ValueError): perception.parse_screen({'image_base64':''})
        with self.assertRaises(Exception): perception.parse_screen({'image_base64':'!!!!'})

    def test_worker_contract_and_exit(self):
        result=subprocess.run([sys.executable,str(ROOT/'omni_server/main.py')],input=b'{}',capture_output=True,timeout=25,check=True)
        data=json.loads(result.stdout)
        self.assertEqual(data['elements'],[])
        self.assertEqual(data['error'],'ValueError')
        self.assertEqual(result.stderr,b'')

    def test_real_ocr_not_mock_coordinates(self):
        from PIL import Image,ImageDraw,ImageFont
        import pytesseract
        pytesseract.get_tesseract_version()
        image=Image.new('RGB',(800,600),'white')
        font=ImageFont.load_default(size=48)
        ImageDraw.Draw(image).text((140,255),'Save File',font=font,fill='black')
        stream=io.BytesIO();image.save(stream,format='PNG')
        result=perception.parse_screen({'image_base64':base64.b64encode(stream.getvalue()).decode()})
        self.assertTrue(any(e['name']=='Save File' for e in result['elements']),result)
        self.assertTrue(any(e['name']=='Save' for e in result['elements']),result)
        for e in result['elements']:
            self.assertGreaterEqual(e['bounds']['x'],130)
            self.assertGreater(e['bounds']['width'],0)
            self.assertEqual(e['source'],'ocr')

if __name__=='__main__': unittest.main()
