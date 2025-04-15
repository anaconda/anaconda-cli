# anaconda-cli

The `anaconda-cli` package encapsulates a set of plugins that make up the Anaconda CLI. This CLI can be used to interact with both anaconda.com and anaconda.org from the command line.

Currently, the following plugins are included as dependencies:

* `anaconda-cli-base`: defines the core CLI, core utilities, and a plugin mechanism
* `anaconda-client`: provides functionality for uploading and managing packages on anaconda.org
* `anaconda-auth`: provides login functionality with both auth.anaconda.com and anaconda.org, token management, a `conda` plugin for authenticated repository access, and a core HTTP client for accessing authenticated APIs
