from fastapi import FastAPI

app = FastAPI(title="{{name}}")


@app.get("/health")
def health() -> dict[str, str]:
    """Tells a caller (and a deploy check) that the service is up."""
    return {"status": "ok"}
