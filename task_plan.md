# Task Plan: Phase 8 Apple-Style UI Refresh & Release Reliability

**Goal**: Deliver the next phase of `visual-automation-designer` as a professional Apple-style desktop UI refresh, then validate, push, and release it without breaking desktop runtime quality gates.
**Started**: 2026-06-09
**Status**: In Progress

## Phases

| # | Phase | Status | Notes |
|---|-------|--------|-------|
| 1 | Discovery and phase definition | [x] complete | Existing roadmap reviewed; current user priority is Apple-style professional redesign, not the older v0.4.x sequencing. |
| 2 | Planning and acceptance criteria | [x] complete | Defined next phase around UI redesign plus release reliability gates. |
| 3 | Release baseline audit | [ ] pending | Must reconcile version mismatch before trusting further release automation. |
| 4 | UI design system redesign | [ ] pending | Update tokens, surfaces, typography, spacing, and visual hierarchy for macOS-inspired desktop feel. |
| 5 | UI implementation slice 1 | [ ] pending | Apply redesign to app shell, toolbar, sidebars, config panel, and canvas chrome. |
| 6 | UI implementation slice 2 | [ ] pending | Apply redesign to block nodes, validation surfaces, execution panels, and remaining states. |
| 7 | Verification | [ ] pending | Run `npm run test`, `npm run lint`, `npm run build`, and release-relevant checks. |
| 8 | Push and release | [ ] pending | Push final phase commits and trigger the next tag-based release only after verification passes. |

## Acceptance Criteria

1. Main desktop surfaces look intentionally Apple-inspired rather than glassmorphism-heavy.
2. Left toolbox, top toolbar, center canvas chrome, and right config panel share one coherent design system.
3. Validation, execution status, and block-node states remain understandable after the visual redesign.
4. `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` versions match before the next release.
5. The phase ends with a verified GitHub push and a newly triggered release workflow.

## Errors Encountered

| Error | Phase | Attempt | Resolution |
|-------|-------|---------|------------|
| Category planning task timed out before start | Planning | 1 | Continued with direct planning using repo evidence already collected. |
| Release version mismatch observed: `package.json` = `1.0.0` while Rust/Tauri config = `1.0.2` | Release baseline audit | 1 | Must fix before the next phase release is cut. |

## Decisions

| Decision | Rationale | Date |
|----------|-----------|------|
| Treat the next phase as an Apple-style UI refresh plus release reliability, not DPI hardening | This matches the latest user priority and the unfinished professional-design ask. | 2026-06-09 |
| Use planning-first execution | The goal is multi-phase: design, implementation, verification, GitHub push, and release. | 2026-06-09 |
| Keep release work bounded behind explicit verification gates | Current version evidence shows release automation is still fragile. | 2026-06-09 |

## Handoff (Current Session → Next Slice)

**Last completed**: Phase definition and planning bootstrap.
**Next action**: Fix release baseline consistency, starting with `F:\projects\visual-automation-designer\package.json` version alignment and a quick verification of release-related scripts.
**Key context**:
- Current release `v1.0.2` is already in progress on GitHub.
- `package.json` still reports `1.0.0`, so the repository is not fully version-consistent.
- The next phase should not cut another release until this baseline is corrected and the UI redesign is implemented and verified.
