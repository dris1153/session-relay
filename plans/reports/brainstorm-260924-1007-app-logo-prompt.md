# Brainstorm: new app logo (AI image prompt)

## Problem
Replace the default Tauri icon with a Session Relay logo. User generates the image with an AI tool, so the deliverable is a prompt.

## Constraints found
- One source image feeds everything: `npx tauri icon <png>` regenerates `src-tauri/icons/*` (ico, png, Store logos). The tray uses the window icon, so it follows automatically.
- Tray attention state (`tray.rs::with_dot`) paints a Clay dot of radius 1/4 icon size in the **bottom-right** corner. The logo must keep its Clay accent out of that quarter, or attention and normal look alike.
- Must read at 16×16 (tray): bold flat shapes, no thin lines or detail.
- DESIGN.md: Carbon Ink `#121212` on Bone Parchment `#f8f8f6`, Clay `#d97757` only as a small accent.
- Unofficial app: avoid Claude/Anthropic marks (starburst, asterisk, sparkle) and a Clay-filled tile, which would read as an official Claude logo.

## Options considered
| Concept | Verdict |
|---|---|
| Two halves passing to each other | Chosen: means "relay between two machines", survives 16 px |
| Relay baton | Reads as a sports icon |
| Folded page + arrows | Generic sync icon |
| Serif S monogram | Fits the style, says nothing about syncing |

Colour: ink + one Clay dot (chosen) · pure monochrome · Clay tile (rejected: looks official).
Shape: rounded-square tile with parchment fill (chosen: visible on light and dark taskbars) · bare glyph (black disappears on a dark taskbar).

## Decision
Prompt in the chat reply. Generated image → user sends it → fix exact colours and rounded-corner transparency if the generator cannot, then `npx tauri icon`, rebuild, check the tray at 100%/150% scale and the attention dot.

## Success criteria
- Recognisable at 16 px in the tray, both normal and with the attention dot
- Corners outside the tile transparent in `icon.ico`
- Only the three palette colours
