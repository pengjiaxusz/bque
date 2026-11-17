<!-- PENGJ_TEMPLATE_START -->

# bque Coding Standards

## General conventions
- Route all builds, checks, tests and benchmarks through `just` (`just check`, `just test`, `just q-run <cmd>`); never run raw `cargo`/`ninja`/`cmake`. See `.agents/skills/bque/SKILL.md`.
- After changes pass verification, commit and push before replying. Skip only for pure Q&A or when reporting failing builds/tests.
- After cloning, run `just doctor --fix` (or `sh .pengj-templates/scripts/doctor.sh --fix` / `pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1 -Fix` when `just` is unavailable): it checks the toolchain, activates the commit hooks, creates the agent-client workspace links (`.trae` / `.claude/skills` / `.workbuddy/skills`), and runs the project fixes in `.pengj-templates/scripts/doctor.d/`. A missing `pengj-templates-cli` is an error, not a warning — the gate and the links both need it — and `--install-cli` is the only step that touches the network (`--fix` never does).
- Commit subjects MUST be pure English (no CJK characters); type/scope stay English. Every commit entry point follows this rule — the commit-msg hook rejects Chinese subjects.
- The commit gate runs from `.githooks/`; activate it with `just doctor --fix` (or `sh .pengj-templates/scripts/doctor.sh --fix`) — the doctor calls `.githooks/install.sh`, which needs only git and a POSIX shell, so activating the hooks does not need the CLI. The gate itself runs from `pengj-templates-cli`: without it the hooks block the commit and print the install command. The gate layer implies the `ci` layer, which runs the same binary as a required status check.
- There is exactly one rule set: the hooks only locate the CLI and hand over, and the template ships no fallback script. A machine without the CLI cannot commit locally until it installs the CLI — never a weaker local check that might disagree with CI. The `gate` job in `.github/workflows/gate.yml` and its ruleset in `.github/rulesets/gate.json` enforce the same rules server-side.
- Check the subject before committing: it must match `type(scope): subject`, the type must be one of `commit.types` in `.pengj-templates/gate.toml`, the subject must carry no Han characters, it must not end with a full stop, and the header stays within 100 characters. The gate enforces all of it, but a self-check is faster than a rejection.
- Code-language guard: every commit checks identifiers and string copy against `.pengj-templates/gate.toml` (`mode`) — English programming rejects non-English, Chinese programming rejects non-Chinese. Chinese mode judges human-facing copy only: command-line flags, paths, protocol field names and embedded source are machine text, not violations (they still count toward the denominator so the budget is not diluted). Each file tolerates a small ratio of string exceptions (default 1%); genuine ones must be certified in source with `lang-ok: <reason>` on the line or `lang-allow: <reason>` near the top. Full audit: `just gate-check`.
- If a change touches module boundaries, public APIs, data flow, lifecycle, or adds a module/subsystem, update the matching architecture/design docs before committing.
<!-- PENGJ_TEMPLATE_END -->

<!-- 项目专属规范可写在下方（锚点之外），模板更新时将完整保留用户内容 -->

