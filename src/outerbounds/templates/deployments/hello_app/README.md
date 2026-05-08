# Hello App

A simple FastAPI application deployed to Outerbounds.

## Endpoints

- `GET /` - Welcome message
- `GET /health` - Health check
- `GET /greet/{name}` - Personalized greeting

## Running Locally

```bash
pip install -r requirements.txt
uvicorn app:app --reload
```

Then visit http://localhost:8000
