# Adding a New Dashboard Widget

This checklist captures the steps to add a new widget type to the dashboard.

## Required
1. **`apps/web/src/lib/widget-types.ts`** — add a `WidgetTypeDefinition` entry. The `sizing` field is required (TypeScript will fail to compile if you omit it).
2. **`apps/web/src/components/dashboard/widget-renderer.tsx`** — add a `switch` case for the new `widget_type`.
3. **`apps/web/src/components/dashboard/widget-picker.tsx`** — register the picker entry (icon + i18n label key).
4. **`apps/web/src/components/dashboard/widget-config-dialog.tsx`** — add a config form if the widget needs configuration.
5. **`apps/web/src/locales/{en,zh}/dashboard.json`** — i18n strings for the picker entry and any in-widget text.

## Optional
- Widget component: `apps/web/src/components/dashboard/widgets/<name>.tsx`
- Vitest coverage: `apps/web/src/components/dashboard/widgets/<name>.test.tsx`

## Picking a `sizing` strategy

| When the widget is …                                  | Use                                              |
| ----------------------------------------------------- | ------------------------------------------------ |
| A chart, table, or generic free-sized panel           | `{ kind: 'free' }`                               |
| A single fixed-size readout (one viable footprint)    | `{ kind: 'fixed' }` (set minW=maxW, minH=maxH)   |
| A radial / gauge / aspect-locked visual               | `{ kind: 'aspect-square', tiers: [...] }`        |
| A list/table that should hug its measured content     | `{ kind: 'content-height' }`                     |

For `aspect-square`, `tiers` is the list of legal coarse-grid sizes (w × w). For gauge today: `[2, 3, 4, 5, 6]` — every integer between minW (2) and maxW (6).

## Related

- Architecture: `docs/superpowers/specs/2026-05-28-dashboard-widget-sizing-strategy-design.md`
- Strategy dispatcher: `apps/web/src/components/dashboard/sizing-strategies/`
