---
phase: 2b
title: "Sticky prompt and open at the end"
status: completed
priority: P2
effort: "0.5d"
dependencies: [2]
---

# Phase 2b: Sticky prompt and open at the end

## Overview
Two reading aids requested after Phase 2: the prompt of the turn being read stays visible at the top (like the Claude VS Code extension), and a session opens at its newest message with a "↓ Latest" button once scrolled away.

## Decisions (user, 2026-09-24)
- Sticky: a compact bar (time + prompt clamped to 2 lines) shown only while the full prompt is scrolled out above; click scrolls back to the prompt; the next turn's bar pushes it up.
- Opening: scroll to the end; stay there while items below settle; "↓ Latest" button after scrolling up.

## Architecture
- `conversation.tsx`: groups visible items into turns (a prompt and everything until the next prompt; items before the first prompt form a turn without one), owns the scroll box and the "Latest" button.
- `turn-section.tsx`: `<li>` with a zero-height `position: sticky` holder (no layout space, no flex gap) and the turn's items; an IntersectionObserver on the full prompt (root = scroll box) shows the bar only when the prompt is above the viewport.
- `src/lib/use-stick-to-bottom.ts`: scroll to the end once the view is ready; a ResizeObserver on the list keeps it there while pinned (within 80 px of the end); the button shows past 600 px.
- Top-level turns only; subagent conversations opened inside keep their plain list.

## Related Code Files
- Create: `src/features/session-viewer/{conversation,turn-section}.tsx`, `src/lib/use-stick-to-bottom.ts`
- Modify: `src/features/session-viewer/session-viewer.tsx`, `src/locales/{en,vi}.json`

## Success Criteria
- [x] Opening a session lands on its last message, also for the 28 MB session (heights settle without drifting up)
- [x] While reading a long reply, the turn's prompt is visible at the top and clicking it returns to the prompt
- [x] "↓ Latest" appears after scrolling up and returns smoothly to the end
- [x] Toggling system events keeps the reading position (or stays at the end when there)

## Risk Assessment
- Estimated heights (`content-visibility: auto`) change as items render → pinned ResizeObserver; measured on the largest real session.
