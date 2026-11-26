# Sprint 8 desktop audit

The same interaction script captured Sprint 7 main and the Sprint 8 implementation at 1440 × 900 and 800 × 650. Chromium renders the application with its test-only desktop bridge. Inventory sizes come from CLI scans of disposable fixtures. The bridge supplies repeatable picker timing, cancellations, a warning and a failed scan. History and export controls were exercised through the bridge. Native Linux WebKit runs separately in CI. Native Windows and macOS GUI interactions and system dialogs require hands-on validation.

| Flow | Sprint 7 observation | Sprint 8 result |
| --- | --- | --- |
| 01–02 First launch and folder selection | Three empty data panels compete with the scan action. | One workspace action, recognizable examples, and local read-only guidance. |
| 03–04 Overview and grouping | Chart ticks and a narrow legend compete for attention. Reviewable and protected totals lack a proportional readout. | A simplified ring, readable values, a proportional reviewability strip and stacked metrics. |
| 05 Export | Controls disable while the Save dialog opens, with no interim feedback. | A status message explains the pending dialog. |
| 06–07 Projects and details | Filters survive drilldown, but keyboard focus is lost on return. | Restore the initiating row and scroll position. Show evidence when it adds information and disable selection for incomplete measurements. |
| 08 Cleanup review | A disabled execution button looks like a future action the user can unlock. | The review ends with its availability in plain text. |
| 09–11 History, settings and comparison | The delta is visible, but readers must find both scan totals in separate controls. Settings are clearly grouped. | The compared sizes sit beside the delta. History tables, controls, and navigation use more consistent typography. |
| 12–13 Compact and error | The navigation rail shrinks appropriately. The first-launch placeholders still dominate after a failed scan. | The rail keeps accessible button names and the error sits above the relevant scan action. |
| 14–18 Progress, warnings, protection and empty results | Progress is terse, and an unscanned workspace resembles an empty result. Protection reasons are available in project detail. | A two-line progress message explains the work. Empty results keep the measured inventory state. Partial results and protection reasons stay visible. |

The keyboard regression test covers the skip link, the single first-launch scan action, and focus and filter restoration at 640 × 480. Existing tests cover cancellation, chart selection, candidate paging, protections, comparisons, and history confirmation. The screenshots do not establish screen-reader behavior or Windows display-scaling compatibility.

## Before and after

| Theme | Before | After |
| --- | --- | --- |
| Dark | [Sprint 7 gallery](screenshots/sprint8/before-dark.jpg) | [Sprint 8 gallery](screenshots/sprint8/after-dark.jpg) |
| Light | [Sprint 7 gallery](screenshots/sprint8/before-light.jpg) | [Sprint 8 gallery](screenshots/sprint8/after-light.jpg) |

[Dark Overview](screenshots/sprint8/overview-dark.png) · [Light first launch](screenshots/sprint8/first-launch-light.png) · [Compact Overview](screenshots/sprint8/compact-dark.png) · [Light history comparison](screenshots/sprint8/comparison-light.png)

To regenerate individual PNG files, run this from `apps/stackrinse-desktop` after setting up the development dependencies and Playwright Chromium.

```sh
STACKRINSE_CAPTURE=/tmp/stackrinse-screenshots npm run test:e2e -- sprint-capture
```
