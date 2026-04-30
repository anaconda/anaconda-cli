# {title}

An Outerbounds project with Anaconda channel support.

## Getting Started

1. Deploy the project:
   ```bash
   ana ob deploy
   ```

## Project Structure

- `flows/hello_flow/` - Example Metaflow workflow with Anaconda conda decorator
- `deployments/hello_app/` - Example FastAPI application
- `src/anaconda_metaflow_extensions/` - Custom Metaflow decorators

## Running Locally

### Run the flow locally:
```bash
cd flows/hello_flow
python flow.py run
```

### Run the app locally:
```bash
cd deployments/hello_app
pip install -r requirements.txt
uvicorn app:app --reload
```

## Anaconda Conda Decorator

This project includes a custom `@anaconda_conda` decorator that configures Metaflow
to use Anaconda's main channel (`https://repo.anaconda.com/pkgs/main`) by default.

```python
from anaconda_metaflow_extensions.conda import anaconda_conda

@anaconda_conda(packages={"numpy": "2.0.0"}, python="3.12")
@step
def my_step(self):
    import numpy as np
    ...
```
