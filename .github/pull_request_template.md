<!-- Pull request checklist. Delete the sections that do not apply. -->

## What changed

## Checklist

- [ ] Every commit subject follows `type(scope): subject`.
- [ ] Every commit subject is written in English; no Han characters in it.
- [ ] The `gate` check is green.


> The check runs on the server, so it needs nothing from your machine. To catch the same problems before pushing, enable the local gate once: `just doctor --fix` (or `pengj-templates-cli gate hooks install`).
