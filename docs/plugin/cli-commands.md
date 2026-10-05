# Plugin commands (the command line)

Like Django's `manage.py` commands: a plugin offers named commands, and an administrator runs any
installed plugin's commands from the command line. They are for work that is done by a person at a
terminal or by a script: importing data, rebuilding an index, fixing records, running a report.

```sh
aether --list-commands --org acme
aether --command currency.import_rates --org acme --arg date=2026-10-05
aether --command tools.greet --org acme --json '{"name": "Ann", "times": 2}'
```

(These are not the *kernel commands* a plugin sends to the kernel, such as `db::create`; those are in
[Kernel commands](commands.md).)

## Declaring a command

In `plugin.toml`, one `[[command]]` per command:

```toml
[[command]]
name = "import_rates"             # lower-case letters, digits, _ and -
function = "import_rates"          # the plugin function it runs
help = "Fetch the exchange rates for a day"
args = [
  { name = "date", help = "YYYY-MM-DD, default today" },
  { name = "days", type = "int", default = 7 },
  { name = "force", type = "bool" },
  { name = "source", required = true },
]
```

A command is an ordinary plugin function, in WebAssembly or Rhai. It gets its arguments as a JSON
object and returns JSON, which is printed.

```rhai
fn import_rates(input) {
    let day = if input.date == () { context::get().now.sub_string(0, 10) } else { input.date };
    // ...
    #{ imported: 12, day: day }
}
```

| Argument key | Meaning |
|---|---|
| `name` | lower-case letters, digits and `_` |
| `type` | `string` (default), `int`, `float`, `bool` or `json`; the command line gives text and the kernel converts it |
| `required` | the command refuses to run without it |
| `default` | used when it is not given, in the argument's own type (a required argument has none) |
| `help` | shown by `--list-commands` |

A manifest with an invalid command (a bad name, an unknown type, a default of the wrong type, two
arguments with the same name) is refused when the plugin is loaded, with a message naming the problem.

## Running one

`--command <plugin>.<command> --org <organization>` with:

* `--arg key=value`, repeated for several. Booleans accept true/false, yes/no, 1/0, on/off. A value
  may contain `=`.
* `--json '{"key": value}'` for the whole input as JSON. It can be combined with `--arg` as long as no
  argument is given twice.

Unknown arguments, missing required ones and values of the wrong type are refused before the plugin
runs, with a message that lists what the command takes. A command that declares no arguments accepts
free-form `--json` and no `--arg`.

The result is printed as JSON and the exit status is 0. If the function fails, its message goes to
standard error and the exit status is 1, so commands work in shell scripts and cron.

## What a command can do

* It runs in the process of the command line; **the server does not need to be running**. It needs the
  same configuration, database and `app_dir` as the server.
* It runs as the kernel's `system:cli:<operating system user>` actor, not as a person, with the plugin's
  **own** capabilities and model grants (a command cannot do more than its plugin can). The plugin must
  be installed and enabled in the organization. Every run is recorded in the plugin call audit.
* It can do slow work in the background instead of making the terminal wait: the function enqueues a
  job with `scheduler::enqueue` and returns its id.
* Commands are plain functions, so everything else that calls functions can call them:
  `plugins::call` from another plugin (Django's `call_command`), the scheduler (a `[[schedule]]` entry
  with the same function), or a background job.

`--list-commands` shows every installed plugin's commands, so one plugin's commands are as easy to
find as another's:

```
other
  shout                   A command from another plugin
tools
  greet                   Say hello a number of times
      --arg name=<string>  who to greet; required
      --arg times=<int>  default 1
```

## How it is stored

The catalog keeps each plugin version's commands in `plugins.commands` (migration
`024_plugin_commands`), filled when a plugin is loaded. Listing and running read the version installed
in the organization, so upgrading a plugin changes its commands for that organization at the same time.
