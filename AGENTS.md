<!-- PENGJ_TEMPLATE_START -->

# bque Coding Standards

## General conventions
- Route all builds, checks, tests and benchmarks through `just` (`just check`, `just test`, `just q-run <cmd>`); never run raw `cargo`/`ninja`/`cmake`. See `.agents/skills/bque/SKILL.md`.
- After changes pass verification, commit and push before replying. Skip only for pure Q&A or when reporting failing builds/tests.
- Commit subjects MUST be pure English (no CJK characters); type/scope stay English. Every commit entry point follows this rule — the commit-msg hook rejects Chinese subjects.
- If a change touches module boundaries, public APIs, data flow, lifecycle, or adds a module/subsystem, update the matching architecture/design docs before committing.
<!-- PENGJ_TEMPLATE_END -->

<!-- 项目专属规范可写在下方（锚点之外），模板更新时将完整保留用户内容 -->

