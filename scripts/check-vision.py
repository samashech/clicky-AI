"""Live local-model evaluation. Synthetic fixtures are test inputs, never runtime targets.
Requires Pillow, Cargo and an explicitly configured local Ollama endpoint/model.
Checks action semantics AND geometric accuracy; no microphone or desktop capture.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]


def fixtures(directory):
    font = ImageFont.load_default(size=22)
    cases = []
    for name, bounds in [('export_left', (80, 170, 260, 240)), ('export_right', (640, 380, 830, 450))]:
        image = Image.new('RGB', (960, 600), '#252933')
        draw = ImageDraw.Draw(image)
        draw.text((25, 25), 'Video editor', fill='white', font=font)
        draw.rounded_rectangle(bounds, radius=8, fill='#f3f4f6')
        draw.text((bounds[0]+35, bounds[1]+23), 'Export', fill='black', font=font)
        path = directory / (name + '.png'); image.save(path)
        truth = [bounds[0]/960*1000, bounds[1]/600*1000, bounds[2]/960*1000, bounds[3]/600*1000]
        cases.append((name, path, 'Click Export', 'click', truth, None, []))
    for complete in (False, True):
        image = Image.new('RGB', (960, 600), '#24262c'); draw = ImageDraw.Draw(image)
        draw.text((25, 25), 'Video editor - sample.mp4', fill='white', font=font)
        draw.rectangle((100, 80, 850, 260), fill='#111111')
        for i in range(11):
            x = 100 + i*70
            draw.line((x, 310, x, 335), fill='white')
            draw.text((x-8, 280), str(i)+'s', fill='white', font=font)
        right = 450 if complete else 660
        draw.rectangle((100, 355, right, 440), fill='#4876ad')
        draw.text((120, 380), 'sample.mp4', fill='white', font=font)
        draw.rectangle((right-10, 355, right, 440), fill='#ffd260')
        name = 'trim_complete' if complete else 'trim_drag'
        path = directory / (name + '.png'); image.save(path)
        history = ['User confirmed: Drag the clip right edge to five seconds. Expected result: the clip ends at the 5s ruler mark.'] if complete else []
        # Allow a small localization margin, while rejecting the 8s/10s positions.
        cases.append((name, path, 'Trim the clip right edge to five seconds.',
                      'complete' if complete else 'drag', None if complete else [665, 580, 700, 745],
                      None if complete else [455, 580, 485, 745], history))
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', choices=['export_left', 'export_right', 'trim_drag', 'trim_complete'])
    args = parser.parse_args()
    for key in ['CLICKY_VISION_ENDPOINT', 'CLICKY_VISION_MODEL']:
        if not os.environ.get(key): parser.error(f'Set {key} explicitly')
    failed = []
    with tempfile.TemporaryDirectory(prefix='clicky-vision-eval-') as directory:
        for name, path, goal, action, target, destination, history in fixtures(Path(directory)):
            if args.case and args.case != name: continue
            env = os.environ.copy()
            for key in list(env):
                if key.startswith('CLICKY_VISION_EXPECT'): env.pop(key)
            env.update(CLICKY_VISION_IMAGE=str(path), CLICKY_VISION_MODE='guide', CLICKY_VISION_GOAL=goal,
                       CLICKY_VISION_EXPECT_ACTION=action, CLICKY_VISION_HISTORY=json.dumps(history))
            if target: env['CLICKY_VISION_EXPECT_TARGET'] = json.dumps(target)
            if destination: env['CLICKY_VISION_EXPECT_DESTINATION'] = json.dumps(destination)
            result = subprocess.run(['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml',
                                     'vision::tests::live_model_image', '--', '--ignored', '--nocapture'],
                                    cwd=ROOT, env=env, timeout=180, capture_output=True, text=True)
            print(json.dumps({'case': name, 'passed': result.returncode == 0}), flush=True)
            print(result.stdout, flush=True)
            if result.returncode:
                print(result.stderr, flush=True); failed.append(name)
    if failed: raise SystemExit('Failed visual acceptance cases: '+', '.join(failed))


if __name__ == '__main__': main()
