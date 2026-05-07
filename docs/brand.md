# bridgething brand

Locked brand for bridgething: wordmark, icon, palette, typography,
voice. The Figma file holds the canonical artwork; this doc holds the
decisions so anything rebuilt from scratch matches.

## voice

### tagline

**the thing. fully open. all yours.**

In display: "the thing." in Outfit Medium, "fully open. all yours." in
Outfit ExtraLight. Echoes the wordmark's weight pattern.

### elevator pitch

> the bridge layer that lets the thing remain itself while opening up
> to anything you build for it.

For READMEs, social bios, and About pages. One sentence; never split.

## wordmark

Always `bridgething`. Always lowercase. Never split, never capitalized,
never wrapped to two lines.

| char range | text   | font   | weight     |
|------------|--------|--------|------------|
| `0..6`     | bridge | Outfit | Medium     |
| `6..11`    | thing  | Outfit | ExtraLight |

Letter spacing: -3% at display sizes, -2% in lockup or body.

## icon

Three-arc bridge inside a device-frame outline, knob overhanging the
right edge. The mark reads as stacked arcs (Spotify wave) and as
literal bridge spans. Both readings are intentional.

### variants

The icon inverts depending on the background it sits on. Frame, middle
arc, posts, baseline, and knob all share the variant's monochrome
color. Top and bottom arcs stay `#00A8E8` in both.

| variant      | sits on  | monochrome color |
|--------------|----------|------------------|
| `icon-light` | light bg | `#1B1F23`        |
| `icon-dark`  | dark bg  | `#EFEFEF`        |

### rules

- Knob always on the right. Never mirror.
- Never place the icon on `#00A8E8`. The brand blue collides with the arcs.
- Maintain clear space equal to the mark's height on all sides.

## color

| token     | hex       | role                                 |
|-----------|-----------|--------------------------------------|
| charcoal  | `#1B1F23` | primary dark, dark-mode background   |
| off-white | `#EFEFEF` | primary light, light-mode background |
| blue      | `#00A8E8` | accent only, never primary or body   |
| soft-gray | `#A7ADB5` | tertiary text, muted UI              |

## typography

| role      | family | weight     | use                                      |
|-----------|--------|------------|------------------------------------------|
| primary   | Outfit | Medium     | wordmark first half, headlines, taglines |
| secondary | Outfit | ExtraLight | wordmark second half, tagline tail       |
| ui / body | Inter  | Regular    | UI surfaces and body text                |

Outfit is SIL OFL via Google Fonts. Inter ships with the kiosk image.

## usage rules

- Always lowercase wordmark.
- Knob always on the right.
- Blue as accent only.
- Invert icon by mode: dark frame on light, light frame on dark.
- Maintain clear space at all sizes.
