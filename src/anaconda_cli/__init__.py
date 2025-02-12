# This is not a real package. We are just keeping it as a package to allow automated
# version number generation via git tag.
try:
    from anaconda_cli._version import version as __version__
except ImportError:  # pragma: nocover
    __version__ = "unknown"


__all__ = ["__version__"]
