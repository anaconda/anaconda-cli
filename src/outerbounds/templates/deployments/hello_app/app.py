from fastapi import FastAPI

app = FastAPI(title="Hello App", description="A simple example API")


@app.get("/")
async def root():
    """Return a welcome message."""
    return {"message": "Hello from Outerbounds!"}


@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy"}


@app.get("/greet/{name}")
async def greet(name: str):
    """Greet a user by name."""
    return {"message": f"Hello, {name}!"}
