# Aether Plugin Documentation

A plugin or addon is a component meant to extend the features of aether.

There are 2 ways to create a plugin:
- Workspace
- Plugin

# Workspace

A workspace is a of grouping a bunch of related plugins.

it is always been checked with the `workspace.toml` file

```toml
[workspace]
name = "example"
label = "Example Workspace"
version = "0.0.1-beta"
description = "This is a description of an example"
long_description = """
This is the long description of an example
"""

```


# Plugin

