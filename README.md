# herdr-plugin-renamer

A [herdr](https://herdr.dev) plugin that names panes and their tabs from a
coding agent's first prompt. In auto-generated linked worktrees, it also renames
the git branch and workspace.

It supports Claude Code, Codex, and Pi. Slugs come from Apple FoundationModels on
supported Macs, then Codex, then a deterministic local fallback.

## Install

```sh
herdr plugin install wyattjoh/herdr-plugin-renamer
```

GitHub installs run the manifest's Rust and platform-specific build steps before
registering the plugin. On macOS, the optional FoundationModels helper is built
only when the active Swift toolchain supports its guided-generation macros;
otherwise installation continues with the Codex/local fallbacks. Local
`herdr plugin link` intentionally does not build; use `just link` from a
development checkout instead.

Install the herdr integration for each agent you use:

```sh
herdr integration install claude
herdr integration install codex
herdr integration install pi
```

## Requirements

- herdr 0.7.4+ on macOS or Linux
- For on-device naming: macOS 26+ on Apple Silicon, Apple Intelligence enabled,
  and a Swift toolchain with FoundationModels guided-generation macro support.
  Current Command Line Tools omit that macro plugin, so installation skips the
  optional helper.
- For Codex naming: the `codex` CLI on `PATH` and logged in

Without either naming model, the plugin derives a rough local slug from the
prompt.

## What it renames

A prompt about reviewing a cache might rename the pane, and the tab holding it,
to `cache-review`. In an auto-generated linked worktree, the plugin can also
rename:

- branch: `<prefix>/cache-review`, or `cache-review` without a prefix
- workspace: `cache-review`

Branch and workspace renaming only happens when the current branch starts with
`worktree/`. The branch rename is local and never pushes to the remote.

Each native agent session is named once, even when its status changes repeatedly.
Session identity—not Herdr's reusable compact pane id—owns the completion and
cached-slug state.

The generated name is also published as pane/workspace metadata:

- pane: `--title`, `--display-agent`, and `$task` token
- workspace: `$task` token

That means the Herdr sidebar and outer title plugins can show the task without
scraping session files. For custom Agent and Space sidebar rows:

```toml
[ui.sidebar.agents]
rows = [["state_icon", "agent"], ["$task"]]

[ui.sidebar.spaces]
rows = [["workspace"], ["$task"]]
```

## Configuration

All settings are optional.

| Setting                       | Default        | Purpose                                      |
| ----------------------------- | -------------- | -------------------------------------------- |
| `HERDR_NAMING_ENGINE`         | auto           | `lm`, `foundation`, or `codex` (see engines below) |
| `HERDR_NAMING_TAB`            | on             | Set to `0` or `false` to keep tab renaming off |
| `HERDR_NAMING_BRANCH_PREFIX`  | none           | Prefix renamed branches, such as `wyattjoh`  |
| `HERDR_NAMING_FOUNDATION_BIN` | bundled helper | Override the FoundationModels helper path    |
| `HERDR_NAMING_CODEX_BIN`      | `codex`        | Override the Codex executable path           |
| `HERDR_NAMING_LM_URL`         | from `lm.json` | Override the LM chat-completions URL         |
| `HERDR_NAMING_LM_MODEL`       | from `lm.json` | Override the LM model name                   |

To configure a persistent branch prefix:

```sh
echo wyattjoh > "$(herdr plugin config-dir herdr-plugin-renamer)/branch-prefix"
```

`HERDR_NAMING_BRANCH_PREFIX` takes precedence over that file. Environment
variables must be available wherever herdr is launched.

## Naming engines

The slug for a rename comes from the first engine that produces one:

1. **LM** (this fork) — a native OpenAI-compatible chat-completion request
   against a private LAN server. Enabled automatically when `lm.json` exists
   in the plugin config dir; leads the default chain on every platform.
2. **Foundation** — on-device Apple FoundationModels (macOS only), with Codex
   as the automatic fallback.
3. **Codex** — headless `codex exec` using ChatGPT-account auth.

Set `HERDR_NAMING_ENGINE` to `lm`, `foundation`, or `codex` to pin one engine;
unset (or unknown) uses the default chain. If every engine in the chain fails,
a deterministic slug is derived from the prompt locally.

### LAN LM setup (`lm.json`)

```sh
herdr plugin config-dir herdr-plugin-renamer
```

Create `lm.json` in that directory:

```json
{
  "url": "https://lm.example.net/v1/chat/completions",
  "resolve": "lm.example.net:443:192.168.50.141",
  "model": "qwen3.8-flash-next-nvfp4",
  "timeout_seconds": 22
}
```

`resolve` is an optional DNS pin (curl's `--resolve` spelling,
`host:port:ip`): traffic for that host goes straight to the IP while TLS SNI
and certificate validation stay on the hostname. `timeout_seconds` defaults
the whole request to 22s; the connect phase is capped at 3s. Validate the
installed setup with:

```sh
"$(herdr plugin list --json | jq -r '.result.plugins[] | select(.plugin_id=="herdr-plugin-renamer") | .plugin_root')/target/release/herdr-plugin-renamer" --lm-test "Add retry to the sync worker"
```

It prints the slug on success and exits non-zero on any config or request
failure.

A step-by-step guide (Chinese) covering install, configuration, smoke testing
and troubleshooting lives in
[docs/lm-engine-tutorial.zh-CN.md](docs/lm-engine-tutorial.zh-CN.md).

## Local development

```sh
just build
just link
```

`herdr plugin link` does not run build steps, so use `just link` or build first.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the complete development and test
workflow.

## License

[MIT](LICENSE)
