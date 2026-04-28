## Project observability

This repo uses the [LSP Brains](https://github.com/KeenanHoffman/LSP-Brains)
methodology for project-state observability. The Brain lives in `.claude/`
and is operated via the `neurogrim` CLI:

```bash
neurogrim score --registry .claude/brain-registry.json
neurogrim narrate --hat visionary --registry .claude/brain-registry.json
```

CLI installation: see
[KeenanHoffman/NeuroGrim](https://github.com/KeenanHoffman/NeuroGrim).
For agent-facing details, see [`CLAUDE.md`](CLAUDE.md).
