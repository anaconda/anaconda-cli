from functools import partial

from metaflow import step, card, current
from metaflow.cards import Markdown
from metaflow.decorators import _base_step_decorator
from metaflow.plugins.pypi.conda_decorator import CondaStepDecorator
from obproject import ProjectFlow

ANACONDA_MAIN_CHANNEL = "https://repo.anaconda.com/pkgs/main"


class AnacondaCondaDecorator(CondaStepDecorator):
    """Step decorator that configures conda with Anaconda's main channel."""
    name = "anaconda_conda"
    defaults = {
        **CondaStepDecorator.defaults,
        "default_channel": ANACONDA_MAIN_CHANNEL,
    }


anaconda_conda = partial(_base_step_decorator, AnacondaCondaDecorator)


class HelloFlow(ProjectFlow):
    """A simple example flow that demonstrates Outerbounds with Anaconda channels."""

    @step
    def start(self):
        """Initialize the flow with a greeting."""
        self.message = "Hello from Outerbounds!"
        print(self.message)
        self.next(self.process)

    @card
    @anaconda_conda(packages={"numpy": "2.0.0"}, python="3.12")
    @step
    def process(self):
        """Process the message using numpy from Anaconda's main channel."""
        import numpy as np

        current.card.append(Markdown(f"# Numpy version: {np.__version__}"))
        current.card.append(Markdown("**Channel:** https://repo.anaconda.com/pkgs/main"))
        print(f"Numpy version: {np.__version__}")

        self.processed = self.message.upper()
        print(f"Processed: {self.processed}")
        self.next(self.end)

    @card
    @step
    def end(self):
        """Finish the flow and display results in a card."""
        current.card.append(Markdown("# Flow Complete"))
        current.card.append(Markdown(f"**Original message:** {self.message}"))
        current.card.append(Markdown(f"**Processed message:** {self.processed}"))
        print(f"Flow complete! Final message: {self.processed}")


if __name__ == "__main__":
    HelloFlow()
