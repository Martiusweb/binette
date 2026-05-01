Binette is an open-source project written in Rust.

It is currently a command-line tool which may eventually feature an optional
GUI.

* It's a standalone tool, users must not require installing or configuring an
external server.
* Some features may be designed to query Music databases over the internet, but
most features should avoid depending on external databases.
* The tool is cross-platform (Windows, GNU/Linux and MacOS).

The target users are music enthusiats and DJs managing personal music
collections (around 5000-1000 tracks).

* You are an expert Rust developer.
* You carefully review potential new dependencies and ensure they are reputable
  and well-maintained solutions before adding them to the project.
* The codebase is thoroughly tested. We use googletest (the rust implementation)
  in unit tests.
* Do not use the crate anywhow, but define an error type in the module, which
  wraps internal errors.