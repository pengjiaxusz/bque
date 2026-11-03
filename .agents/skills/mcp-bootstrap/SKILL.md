---
name: mcp-bootstrap
description: >-
  Project-local MCP bootstrap: install server binaries, verify them with a real stdio handshake, and keep every client config derived from one .mcp.json. Node built-ins plus uv/git only — no external CLI required.
  Triggers: mcp, mcp doctor, mcp install, mcp bootstrap, mcp check, mcp sync, install mcp, verify mcp, mcp not working, renderdoc-mcp unavailable.
---

<!-- PENGJ_TEMPLATE_START -->

# MCP bootstrap — install & verify inside the project

MCP servers are usually configured per client, and many clients only read a global config. This skill keeps the project able to install and verify its own MCP dependencies, without depending on any external tool or repository.

Script: `.agents/skills/mcp-bootstrap/scripts/ensure-mcp.mjs` (Node ≥ 20). Run it from the project root; it also walks up to find `.mcp.json`.

## Three commands

| Command | What it does |
|---|---|
| `node .agents/skills/mcp-bootstrap/scripts/ensure-mcp.mjs` | `doctor` — resolve every command, then a real `initialize` + `tools/list` handshake; exit 1 when something is broken |
| `... install [name]` | Install what is missing (idempotent). `--global` also writes client-wide configs |
| `... sync [--write]` | Regenerate the per-client configs from `.mcp.json` |

`doctor --static` skips spawning processes and only checks paths.

A project without `.mcp.json` is treated as "no project-level MCP": all three commands exit 0 with a notice. Create the file only when you actually add a server.

## Single source of truth

- `.mcp.json` — the declaration; commands are bare names so the file stays portable.
- Derived (never hand-edit): `.agents/mcp.json`, `.trae/mcp.json` (`url` form) and `.agents/mcp_config.json`, `.trae/mcp_config.json` (`serverUrl` form — Antigravity-style clients reject `url`).
- Change `.mcp.json`, then run `sync --write`.

## Adding a server

1. Add an entry to `.mcp.json` (bare command name, no absolute paths).
2. Needs a real install or a side artifact? Add a recipe — either the built-in table in the script, or `scripts/mcp/recipes.local.json` (same shape, project-owned, survives template updates). Pure `npx` / `uvx` servers need no recipe.
3. `sync --write` → `install` → `doctor`.

Recipe fields: `类型` (`uv-git` installs via `uv tool install`; `none` means already on PATH), `源` (`git+https://…`), `入口` (executable name), `扩展` (`从` / `到`), `启用` (`文件` / `键` / `值`), `要求` (host processes that must be closed). Paths accept `~`, `%VAR%`, `$VAR`.

## Red lines

- Never hand-edit derived configs; they are generated and reviewed as such.
- Do not write project-specific MCP into global configs. `--global` is opt-in on purpose.
- A green handshake means the server answers — not that it has data. RenderDoc MCP still needs a `.rdc` loaded in RenderDoc.

<!-- PENGJ_TEMPLATE_END -->

## Project-specific notes (preserved across template updates)

- **MCP servers enabled here**: none yet — without `.mcp.json`, the script treats the project as having no project-level MCP and exits 0.
- **Extra recipes**: add one to `scripts/mcp/recipes.local.json` when a server needs a real install; pure `npx` / `uvx` servers need none.
- **Environment notes**: none.
