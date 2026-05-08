# Hello Flow

A simple Outerbounds workflow that demonstrates the Anaconda conda decorator.

## Features

- Uses `@anaconda_conda` decorator to pull packages from Anaconda's main channel
- Demonstrates Metaflow cards for visualizing results

## Running Locally

```bash
python flow.py run
```

## Running on Outerbounds

```bash
python flow.py --with kubernetes run
```

## Anaconda Conda Decorator

The `@anaconda_conda` decorator configures the step to use Anaconda's main channel:

```python
@anaconda_conda(packages={"numpy": "2.0.0"}, python="3.12")
@step
def my_step(self):
    import numpy as np
    ...
```
