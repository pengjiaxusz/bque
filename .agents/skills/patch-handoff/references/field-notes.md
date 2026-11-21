# Field Notes — One Series, Landed

The concrete details below come from a single landed handoff in a Rust workspace: 12 patches, 12 tickets. The code names are project-specific; the **patterns** repeat in almost every package of this kind.

## 1. The dominant failure: the package targets an imaginary API

| What the package wrote | What the repo actually has | Compiler message |
| :--- | :--- | :--- |
| escaped char literal written as one backslash | needs the double-backslash form | `character literal may only contain one codepoint` + `unterminated character literal` |
| `fn f(text: &str, ...) -> &str` | needs a lifetime tied to `text` | `missing lifetime specifier` |
| `style.size` | `style.font_size` | `no field 'size'` |
| `State::Normal` | `State::None` (variants: `None` / `Error` / `Warning`) | `no variant named 'Normal'` |
| `rect.contains(p)` | `contains_point(p)` / `contains_rect(&r)` | `no method named 'contains'` |
| `place_popup(anchor, size, placement, viewport, gap, offset)` | `(anchor, popup_size, viewport, placement)` | `takes 4 arguments but 6 were supplied` |
| outer type derives `Eq` while containing a type with only `PartialEq` | drop `Eq` from the outer type | `... : Eq is not satisfied` |

**Rule of thumb.** When the errors cluster as "method / field / variant does not exist", fix the call site to match the repo — never add methods to a shared public type to accommodate a patch. When the error is a trait bound, remove the extra derive from the *new* type rather than loosening the shared one.

## 2. Borrow conflicts have one fixed shape

A helper returning `Vec<&str>` borrows `self`; mutating `self` while that borrow is alive fails with `cannot assign to self.x because it is borrowed`.

```rust
// wrong: borrow alive across the mutation
let lines = self.split_lines();
self.cursor_line -= 1;
self.cursor_col = Self::line_char_len(lines[self.cursor_line]);

// right: take the scalars inside a block, mutate after the borrow ends
let len = {
    let lines = self.split_lines();
    Self::line_char_len(lines[target])
};
self.cursor_line = target;
self.cursor_col = self.cursor_col.min(len);
```

When one movement needs two derived values (line length and line count), return a tuple from the block.

## 3. Lint details that block a push

- `let mut x = Vec::with_capacity(n)` that is only moved into a struct and never pushed → drop the `mut`.
- Unused `Result` / `.unwrap()` in non-test code → handle explicitly; the gate is not lenient.

## 4. Fold precondition, concretely

`cp final → git add → commit --amend` is valid only when the file was touched by one commit in the series:

```bash
git log --oneline <base>..HEAD -- src/<file>.rs | wc -l   # must be 1
```

Above 1 means the repair belongs in a separate final commit, with the mapping break stated in the report.

## 5. The gap tests never see

A popup reserved capacity for its menu labels and never pushed any of them. Every shape test passed, because the assertion compared **element counts** across 24 states — a blank row still counts as a row. The menu looked empty to a human and perfect to the suite.

So: after the gates are green, open the new output path and look at it. Text, icons, separators, side effects. Then put the finding in the report and let the user decide whether it blocks landing.

## 6. Numbers worth recording

Record the passing-test total of the run you landed. Next time, a handoff claiming "all tests pass" can be compared against a real number instead of a vague adjective.
