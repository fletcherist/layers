---
name: update-docs
description: Update the user guide (docs/user-guide.md) to reflect the current state of the codebase
disable-model-invocation: true
allowed-tools: Read, Grep, Glob, Bash(find:*), Bash(wc:*), Edit, Agent
---

# Update User Guide

Update `docs/user-guide.md` to accurately reflect the current state of the Layers DAW codebase.

## Process

1. **Discover current features** — Search `src/` for keyboard shortcuts, mouse handlers, UI panels, audio/MIDI features, settings, regions, plugins, automation, components, and any new functionality not yet documented.
2. **Read the existing guide** — Read `docs/user-guide.md` in full.
3. **Diff against reality** — Identify:
   - New features or shortcuts missing from the guide
   - Removed or renamed features that should be deleted or updated
   - Changed behavior (different defaults, new options, renamed settings)
   - New sections needed for entirely new feature areas
4. **Edit the guide** — Apply changes using the Edit tool. Preserve the existing tone (friendly, concise, end-user-facing, no developer jargon) and formatting conventions (GitHub-flavored markdown, tables for shortcuts, `> **Tip:**` callouts).
5. **Keep structure consistent** — If adding a new top-level section, also add it to the Table of Contents with a working anchor link.

## Rules

- Write for end users, not developers. No struct names, function names, or file paths.
- Use macOS-style modifier symbols: `⌘` (Cmd), `⇧` (Shift), `⌥` (Alt/Option).
- Every shortcut table entry needs both the key combo and a plain-English description.
- Do not remove sections unless the feature is fully gone from the codebase.
- If unsure whether a feature exists, grep for it before documenting or removing it.
