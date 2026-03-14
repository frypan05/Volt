from fastapi import FastAPI

app = FastAPI()


@app.get("/items")
async def items():
    return {"ok": True}
