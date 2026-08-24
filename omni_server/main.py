from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
import base64

app = FastAPI(title="Local UI Parser")

class ScreenRequest(BaseModel):
    image_base64: str

@app.post("/parse")
async def parse_screen(req: ScreenRequest):
    try:
        # Mocked output representing OmniParser's JSON structure
        detected_elements = [
            {"id": 0, "type": "button", "text": "File", "box": [10, 10, 50, 20]},
            {"id": 1, "type": "icon", "text": "Crop Tool", "box": [120, 450, 40, 40]},
            {"id": 2, "type": "text", "text": "Save", "box": [180, 450, 60, 20]},
        ]
        return {"elements": detected_elements}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="127.0.0.1", port=8000)
