"""Bounded, one-request local OCR worker. JSON stdin/stdout; no listening port.

Tesseract and Pillow/pytesseract are real optional runtime dependencies.
Missing dependencies are reported, never replaced by synthetic detections.
"""
import base64
import io
import json
import sys

MAX_INPUT = 12 * 1024 * 1024
MAX_PIXELS = 24_000_000


def parse_screen(req):
    from PIL import Image, ImageOps
    import pytesseract
    encoded = req.get("image_base64", "")
    if not encoded or len(encoded) > MAX_INPUT:
        raise ValueError("image payload missing or too large")
    raw = base64.b64decode(encoded, validate=True)
    Image.MAX_IMAGE_PIXELS = MAX_PIXELS
    image = Image.open(io.BytesIO(raw))
    if image.width * image.height > MAX_PIXELS:
        raise ValueError("image exceeds pixel budget")
    original = image.size
    image.thumbnail((1600, 1200))
    sx, sy = original[0] / image.width, original[1] / image.height
    data = pytesseract.image_to_data(ImageOps.grayscale(image), output_type=pytesseract.Output.DICT, config="--psm 11", timeout=12)
    elements = []
    # Group words into lines so controls such as 'Save As' remain matchable.
    lines = {}
    for i, text in enumerate(data["text"]):
        confidence = float(data["conf"][i]) / 100
        if not text.strip() or confidence < .5:
            continue
        key = tuple(data[k][i] for k in ("block_num", "par_num", "line_num"))
        lines.setdefault(key, []).append(i)
    groups = list(lines.values())
    # Menubars often become one OCR line. Preserve whole labels and individual words.
    groups += [[i] for indices in lines.values() if len(indices) > 1 for i in indices]
    for indices in groups[:500]:
        x = min(data["left"][i] for i in indices)
        y = min(data["top"][i] for i in indices)
        right = max(data["left"][i] + data["width"][i] for i in indices)
        bottom = max(data["top"][i] + data["height"][i] for i in indices)
        elements.append({"id": str(len(elements)), "role": "text", "name": " ".join(data["text"][i] for i in indices)[:300],
            "bounds": {"x": x*sx, "y": y*sy, "width": (right-x)*sx, "height": (bottom-y)*sy},
            "confidence": min(float(data["conf"][i])/100 for i in indices), "source": "ocr", "actionable": False})
    return {"elements": elements}


def main():
    try:
        payload = sys.stdin.buffer.read(MAX_INPUT + 1)
        if len(payload) > MAX_INPUT:
            raise ValueError("request exceeds byte budget")
        result = parse_screen(json.loads(payload))
    except Exception as exc:
        # Do not echo request contents or screenshot bytes into diagnostics.
        result = {"error": type(exc).__name__, "elements": []}
    sys.stdout.write(json.dumps(result))

if __name__ == "__main__":
    main()
