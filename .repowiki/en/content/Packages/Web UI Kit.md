# Web UI Kit

<cite>
**Referenced Files in This Document**
- [packages/web-ui/package.json](file://packages/web-ui/package.json)
- [packages/web-ui/src/index.ts](file://packages/web-ui/src/index.ts)
- [packages/web-ui/src/styles.css](file://packages/web-ui/src/styles.css)
- [packages/web-ui/src/shell.css](file://packages/web-ui/src/shell.css)
- [packages/web-ui/src/lib/utils.ts](file://packages/web-ui/src/lib/utils.ts)
- [packages/web-ui/src/components/ui/button/button.svelte](file://packages/web-ui/src/components/ui/button/button.svelte)
- [packages/web-ui/src/components/ui/badge/badge.svelte](file://packages/web-ui/src/components/ui/badge/badge.svelte)
- [packages/web-ui/src/components/ui/input/input.svelte](file://packages/web-ui/src/components/ui/input/input.svelte)
- [packages/web-ui/src/components/ui/card/card.svelte](file://packages/web-ui/src/components/ui/card/card.svelte)
- [apps/web/src/app.css](file://apps/web/src/app.css)
- [tests/contract/shell-geometry.test.ts](file://tests/contract/shell-geometry.test.ts)
</cite>

## Table of Contents

1. [Purpose](#purpose)
2. [Exports](#exports)
3. [Components](#components)
4. [Token Layer](#token-layer)
5. [Shell Geometry](#shell-geometry)
6. [Consumers](#consumers)
7. [Geometry Test](#geometry-test)
8. [Gaps](#gaps)

## Purpose

`@ssh-tunnel/web-ui` is the shared Svelte 5 component library for the console.
It is host-free (no `node:*`, no DOM access at import time) so the same
components render in a browser tab, a Tauri WebView, the DSH panel and the VS
Code webview. It is built in the shadcn-svelte shape: `bits-ui` primitives,
`tailwind-variants` for variants, `clsx` + `tailwind-merge` for class merging.

**Section sources**

- [packages/web-ui/package.json](file://packages/web-ui/package.json#L1-L30)
- [packages/web-ui/src/index.ts](file://packages/web-ui/src/index.ts#L1-L12)

## Exports

```ts
export { Button, buttonVariants, type ButtonProps, type ButtonSize, type ButtonVariant }
export { Input }
export { Badge, badgeVariants, type BadgeVariant }
export { Card }
export { cn, type WithElementRef, type WithoutChild, type WithoutChildren, type WithoutChildrenOrChild }
```

Stylesheets are exported as package subpaths:
`@ssh-tunnel/web-ui/styles.css` and `@ssh-tunnel/web-ui/shell.css`.

**Section sources**

- [packages/web-ui/src/index.ts](file://packages/web-ui/src/index.ts#L1-L12)
- [packages/web-ui/package.json](file://packages/web-ui/package.json#L10-L16)

## Components

| Component | Variants / behaviour |
|---|---|
| `Button` | variants `default`, `secondary`, `outline`, `ghost`, `destructive`; sizes `default`, `sm`, `xs`, `lg`, `icon`, `icon-sm`, `icon-xs`; renders `<a>` when `href` is set, else `<button>` |
| `Badge` | variants `default`, `secondary`, `success`, `warning`, `danger`, `muted`, `outline`; maps to `--signal-running`, `--signal-degraded`, `--signal-failed`; renders `<a>` when `href` is set |
| `Input` | one text-field surface; macOS compact variant (`h-7`, 5px radius, inner shadow, cyan focus ring) |
| `Card` | container surface |

Platform variants are Tailwind custom variants defined per `data-interface`
value: `macos:`, `windows:`, `linux:`. That is how a single component gets a
5px macOS button and a 9px Linux button without separate files.

**Section sources**

- [packages/web-ui/src/components/ui/button/button.svelte](file://packages/web-ui/src/components/ui/button/button.svelte#L1-L40)
- [packages/web-ui/src/components/ui/badge/badge.svelte](file://packages/web-ui/src/components/ui/badge/badge.svelte#L1-L35)
- [packages/web-ui/src/components/ui/input/input.svelte](file://packages/web-ui/src/components/ui/input/input.svelte#L1-L30)

## Token Layer

`styles.css` imports Tailwind 4 once and declares:

- `@custom-variant dark`, plus `macos`/`windows`/`linux` variants tied to
  `[data-interface="…"]`;
- a neutral `:root` token set: radius, scrollbar tokens, glass rim/edge/shade/
  active, and OKLCH `background`, `foreground`, `card`, `popover`, `primary`,
  `secondary`, `muted`, `accent`, `border`, `input`, `ring`, and signal colours.

Hosts import it once and add `@source` for the package, because Tailwind ignores
`node_modules` and would otherwise drop utilities used only inside the library.
`apps/web/src/app.css` does exactly that:

```css
@import "tailwindcss";
@import "@ssh-tunnel/web-ui/styles.css";
@import "@ssh-tunnel/web-ui/shell.css";
@source "../../../packages/web-ui/src";
```

The comment at the top of `styles.css` refers to separate `theme.css` and
`platform.css` files; those files do not exist. The tokens and platform
overrides currently live in `styles.css` itself.

**Section sources**

- [packages/web-ui/src/styles.css](file://packages/web-ui/src/styles.css#L1-L60)
- [apps/web/src/app.css](file://apps/web/src/app.css#L1-L15)

## Shell Geometry

`shell.css` is the frame every host renders: toolbar / body / sidebar / content /
status bar. Geometry lives here; colours come from tokens.

| Element | macOS | Windows | Linux |
|---|---|---|---|
| toolbar height | 56px | 40px (no bottom border) | 56px (drop shadow) |
| toolbar inset | 84px left when native chrome is on (traffic lights) | — | — |
| sidebar rows | 34px | — | — |
| controls | 30px | — | — |
| status bar | 36px, chrome tinted | — | — |

Selectors are driven by two document attributes: `data-interface`
(`macos`/`windows`/`linux`) and `data-native-chrome` (`true` in the desktop
shell). The Web console sets both in `+layout.svelte` from the user agent and
`location.protocol === "tauri:"`.

**Section sources**

- [packages/web-ui/src/shell.css](file://packages/web-ui/src/shell.css#L1-L120)
- [apps/web/src/routes/+layout.svelte](file://apps/web/src/routes/+layout.svelte#L1-L40)

## Consumers

- `apps/web` imports both stylesheets and every component it needs.
- The Tauri shell embeds `apps/web/dist`, so it inherits the kit without a
  separate dependency.
- The DSH panel and VS Code webview frame the same built console; the VS Code
  extension points an `<iframe>` at the daemon, and DSH stages `apps/web/dist`
  under `integrations/dsh/web/`.

**Section sources**

- [apps/web/package.json](file://apps/web/package.json#L14-L30)
- [integrations/dsh/scripts/build.ts](file://integrations/dsh/scripts/build.ts#L1-L25)
- [integrations/vscode/src/extension.ts](file://integrations/vscode/src/extension.ts#L100-L115)

## Geometry Test

`tests/contract/shell-geometry.test.ts` reads `packages/web-ui/src/shell.css`
and asserts the values that make the console match the space-lens reference:
bar heights, row metrics and type sizes. The reference numbers are copied, not
imported, because the sibling repo is not a workspace dependency.

**Section sources**

- [tests/contract/shell-geometry.test.ts](file://tests/contract/shell-geometry.test.ts#L1-L40)

## Gaps

1. **No component-level tests.** The kit is verified indirectly by the Web
   app's build, svelte-check, the e2e selectors (`.tunnel-toolbar`,
   `.tunnel-sidebar`, `.tunnel-content`) and the geometry test.
2. **`styles.css` header names files that do not exist** (`theme.css`,
   `platform.css`).
3. **`badge.svelte` uses `as unknown as`** to widen `restProps`, contrary to the
   `AGENTS.md` convention.
4. **Only four components.** Dialogs, menus and toasts used by `apps/web` are
   implemented locally in `apps/web/src/lib`, not shared here.

**Section sources**

- [packages/web-ui/src/styles.css](file://packages/web-ui/src/styles.css#L1-L12)
- [packages/web-ui/src/components/ui/badge/badge.svelte](file://packages/web-ui/src/components/ui/badge/badge.svelte#L40-L48)
- [apps/desktop/e2e/specs/app.js](file://apps/desktop/e2e/specs/app.js#L1-L40)
