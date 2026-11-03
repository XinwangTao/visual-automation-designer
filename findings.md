# Findings

## Existing roadmap is stale relative to current user priority — 2026-06-09

**Source**: `F:\projects\visual-automation-designer\PROJECT_PLAN.md`, `IMPLEMENTATION_PLAN.md`, `OPTIMIZATION_ROADMAP.md`
**Relevance**: The repo roadmap still centers v0.4.x-era priorities such as DPI correction, control-flow semantics, and release hardening. The current user priority has shifted to a professional Apple-style UI redesign followed by development and release.

Key finding:
- The next phase should be reframed around UI system redesign, not simply continuing the old roadmap order.
- DPI/multi-monitor hardening remains important, but it is not the immediate user-directed next phase.

---

## Release version mismatch is real and currently risky — 2026-06-09

**Source**: `F:\projects\visual-automation-designer\package.json`, `F:\projects\visual-automation-designer\src-tauri\Cargo.toml`, `F:\projects\visual-automation-designer\src-tauri\tauri.conf.json`
**Relevance**: The next release phase cannot be trusted until versions are consistent across all required sources.

Observed values:
- `package.json` → `1.0.0`
- `src-tauri/Cargo.toml` → `1.0.2`
- `src-tauri/tauri.conf.json` → `1.0.2`

Implication:
- The current repo state violates the project’s own release consistency rule.
- The first bounded execution slice should fix this baseline before further release work.

---

## Current release state — 2026-06-09

**Source**: GitHub Actions status check via `gh run list --workflow=release`
**Relevance**: A release is already running, so the next phase must not assume a clean post-release baseline until that workflow completes.

Key finding:
- Release workflow for `v1.0.2` is currently `in_progress`.
- The next-phase planning should treat that as an external moving part, not as proof that release hygiene is complete.

---

## Proposed next phase definition — 2026-06-09

**Source**: User priority + repo evidence
**Relevance**: Provides the planning anchor for the goal-driven workflow.

Proposed phase name:
- **Phase 8: Apple-Style UI Refresh & Release Reliability**

Scope:
1. redesign the core desktop visual system
2. implement the redesign across primary surfaces
3. preserve runtime clarity for validation and execution states
4. repair release-baseline consistency before cutting the next release

First bounded slice:
- Align version metadata and audit release hooks before touching broader UI implementation.
