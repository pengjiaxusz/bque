<!-- PENGJ_TEMPLATE_START -->

# bque Coding Standards
- **Build and test**: Run `just check` / `just test`.
- **Commit before reply**: Stage only your own paths and commit & push them before replying, following each repository's own commit workflow; never ask whether to commit. When the worktree still carries someone else's uncommitted edits, commit your part but do not push and do not touch those files. For multi-repo changes (submodules, downstream template syncs) commit every repository separately, children before the parent pointer (conventional commit: `type(scope): subject`).
- **Code comments**: Comments must be in English.
- **Code-language guard**: English code and strings only (no foreign script); exceptions use `lang-ok` / `lang-allow`, audit with `just gate-check`.
<!-- PENGJ_TEMPLATE_END -->

<!-- 项目专属规范可写在下方（锚点之外），模板更新时将完整保留用户内容 -->

