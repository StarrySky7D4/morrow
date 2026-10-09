# v32 Glass / Music device checklist and source alignment

2026-10-09. Git source baseline `62a8b964bb27840f0348bb6b8ef520142880b9b1`; this report covers additional uncommitted v32 drawing changes. Root owns SDK compilation, immutable HAP selection, installation and device interaction. No device action or SDK build was issued by this audit/implementation task.

The extra global raised contours were a confirmed source-design difference: Flutter's ordinary `Glass` uses its material border and outer shadows with `NeumorphicSurface(depth: 0)`. HMOS added `SurfaceRelief` on top of the border and shadow of ordinary navigation, cards, music and other panels. v32 removes those extra contours from ordinary `Index.Rim` and the candidate material preview. This is a concrete candidate correction for the unknown-frame complaint, not device proof that the complaint is closed.

The actual Flutter search `Glass` is `recessed: true`, so its designed inset remains. Search and music now use one `RecessedGlassRelief.ets`, extracted from the actual v31 music inset painter. Search supplies widget-depth multiplier `-1` with its styled/local Glass depth and outer radius; music supplies `-0.8` with global Appearance depth/radius/palette. The music outer Glass still keeps its component material overrides. No business, file, library or playback method was changed.

## Authoritative references and final source

- Flutter `build/win-cloud-20261005/lib/appearance.dart:476-558,595-702`: inherited `GlassMaterial`, `_styleGlass` borders/casts, local component styling and ordinary depth `0` versus recessed `-1`.
- Flutter `lib/main.dart:2299`: the search is recessed. `lib/music/music_panel.dart:236,274,516`: global `AppearanceScope` supplies the music child palette; the two insets have widget depth `-0.8` and default `fill=false`.
- Flutter `lib/neumorphic_controls.dart:15,186,347-508` and `lib/experimental_controls.dart:171-408`: signed depth, separate strength/opacity, transparent centers, contour parameters and blurred complement openings.
- Flutter `lib/liquid_glass.dart:15-50,325-411` and `lib/surface_paint_boundary.dart:1-28`: liquid rim, exclusively outside shadow paint and the 6-logical-pixel paint budget.
- Actual API26 SDK `component/canvas.d.ts:269-328,634-654,1543-1596,1760-1811,2265-2290,2753-2811`: Canvas/Path2D coordinates default to VP; `filter='blur(2vp)'` / `'blur(4vp)'` are explicitly supported Gaussian filter lengths; filter is write-only; save/restore owns drawing attributes and clipping. Native filter state must not be inferred by reading `ctx.filter`.
- SDK `component/common.d.ts:7484-7606,21070-21088`: general component `shadow` accepts one `ShadowOptions|ShadowStyle`; radius and offsets use physical px; `fill` defaults false. `api/@ohos.arkui.UIContext.d.ts:5140` supplies `vp2px`. `MultiShadowOptions` is not a general Glass multi-cast interface.

All ten Flutter/SDK reference hashes are captured in `glass-style-alignment-a2-inputs-before.json` and `glass-style-alignment-a2-inputs-after.json`. Final production inputs:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `Index.ets` | 338531 | `0930a469a4dd92e98db914e40fb464763a7ff0a0c4a5678e325f72a62684d773` |
| `MusicPanel.ets` | 26020 | `0e0cbb65d2056de3595a9f3f7203120a0e0b689c72cf179ba6dfe080f00f5319` |
| `RecessedGlassRelief.ets` | 5680 | `1ed9f9f5089f7a3f8ff92cf4942d379af5863ab290131e62058ea173e06c0c75` |

`surfaceShadow` preserves the available primary design cast, converts its radius/offsets through the current `UIContext.vp2px`, and explicitly leaves `fill=false`. Flat frosted versus clear uses the inherited 18/14 logical blur radius and 8/4 logical vertical offset, with black casts in dark mode. Flat liquid has its optical rim and no inherited ordinary shadow, matching `GlassMaterial.liquid`; explicit non-flat visual styles retain their cast in liquid mode. Non-flat recessed search has no outer style cast. The candidate material preview now passes `previewMode()` before applying that candidate, so its flat clear/liquid shadow does not accidentally use the saved global mode.

## Screenshot observations

Use the actual installed immutable package identity before attributing a frame to these source changes. Root's dev21/v31 baseline and `device/dev20-before-update.png` do not contain v32. The existing dev20 image shows the narrow home/search/hero/card region only; it does not establish music inset, selected row, dark mode or seven-style acceptance.

Pair each screenshot with the actual UI tree, app/build identity, style, mode, theme, depth, radius, viewport/density and relevant local music/search overrides. Observe two settled frames after layout or a style/radius change; a successful click or elapsed 16 ms alone does not prove Canvas rendered at the new bounds. A screenshot proves the observed region, not hidden routes or unvisited styles.

| View / state | Required observation | Failure cue |
| --- | --- | --- |
| Ordinary frosted/clear home, navigation, summary, card, appearance, daily, music | One intentional material edge and its outside cast. Look across all visible panels, not music alone. | Nested parallel rings, an inset rectangle floating inside the padded content, or a stale-sized frame after resize. |
| Flat music, current track and selected row | No local relief frame or ink-alpha background; outer material remains visible through the row center. Cover art and the play button may keep their own designed fills. | Local boxes around the 64-VP current strip or 40-VP selected row that survive switching to flat. |
| Neumorphic / clay music and search | Soft upper-left shade and lower-right light from an actual rounded opening. The center stays free of a solid added fill. Search is `-1`, music `-0.8`, both multiplied by their respective material/global depths. | Several hard nested rectangles used as shadow substitutes, uniform opaque row tint, or blur bleeding over the entire panel/content. |
| Paper music | Fine dark contour with a light-to-dark inner edge, not thick industrial bands. | Missing inner effect or a heavy multi-ring frame. |
| Fluent music | Dark outer inset contour, shadow-to-light inner edge; radius tracks the style's .55 global scale. | Reversed raised highlight, generic .8 inset geometry without the Fluent branch, or center fill. |
| Brutalist music | Theme ink 1.8-VP contour and a one-sided ink-to-transparent inner edge; small .18-scaled global corners. | Large rounded neumorphic rings or a translucent filled selected-row rectangle. |
| Industrial music | Dark wall, shadow-to-light edge and separate thin inner highlight; .4-scaled global corners. | Missing one of the three designed inset contours or four/six generic rings. |
| Recessed search in each non-flat style | The shared actual inset remains, outer style cast is absent; its own local depth/radius overrides are respected. | Search becomes a plain zero-depth panel or retains the previous positive-raised approximation. |
| Depth zero | Shared inset clears; ordinary material border can remain. | Old inset survives after reducing depth to zero. A normal Glass border alone is not a failure. |
| Music local override only | Local music outer radius/depth changes without changing the global child inset radius/depth. Search local overrides intentionally change its styled inset. | Music child adopts local outer values, unlike Flutter's global music palette. |
| Default Footer / custom Footer | Default Footer stays transparent and unframed; customized material may have its intended Glass edge. | A new global Canvas rectangle appears around default Footer. |
| Liquid mode | Optical rim remains; ordinary raised relief does not return. Additional inset exists only for actual search/music children. | Generic SurfaceRelief rings stack with the liquid rim on every panel. |
| Candidate material preview | Non-recessed preview has no extra relief; candidate clear/liquid shadow uses the candidate mode before application. | Preview shows a saved-global frosted shadow/ring despite a clear/liquid candidate. |

For selected-row screenshots, use only an actual Native Ready track admitted through the real import/recovery path. Empty-list visuals do not qualify selected-row paint. Preserve original requests, spools and the existing editor/draft state; do not fabricate Ready records or replay an Unknown import for visual convenience.

Lifecycle observations: expand/collapse the retained playlist, switch selected track, scroll the same retained list, open/close settings, switch styles back to flat, change global depth/radius, change relevant local material, and resize/rotate where supported. Check that the old selected row's effect disappears, only the actual selected row has an inset, scroll controls still work through decoration-only `HitTestMode.None`, and neither collapse nor disappearance leaves a delayed frame over another view. The current strip remains 64 VP and rows 40 VP; radius must fit the actual rounded region at all widths.

## Canvas lifecycle and remaining risks

Each shared inset component owns one Canvas context. All six primitive paint props (style, depth, signed multiplier, dark, surface, radius) redraw; each draw clears prior bounds. Resizes coalesce into one 16-ms callback and disappearance cancels that timer and clears its identity. The unshifted rounded clip encloses both opposing complement walls; evenodd fill leaves the shifted opening empty; save/restore encloses VP Gaussian filters. Path radii are bounded to available size. These are source properties and controlled-call observations, not native raster proof.

The retained collapsed playlist can retain hidden Canvas nodes. The SDK explicitly warns that invisible Canvas drawing commands queue until a rendering frame; the current helper draws only on primitive changes/resize, not every playback position update. Rapid repeated style/radius changes while collapsed or off-screen still need a device observation for stale-frame flashes or queued work. No continuous timer loop was found in the shared inset helper.

`Index.Rim` still uses measured panel dimensions from `rememberSurface`, initially zero until the real area callback. Layout changes must be observed for the final bounds. Normal Rim now contains no SurfaceRelief; Liquid `GlassRim` retains its own Canvas/timer lifecycle. Explicit style-picker control samples still use the pre-existing SurfaceRelief control approximation; that is distinct from an ordinary zero-depth Glass outer edge and remains outside this correction.

Secondary outer casts remain **OPEN** for neumorphism (upper-left white, offset -1.5/-1.5, blur 5), clay (upper-left light, offset -1/-1.5, blur 3) and industrial (top light, offset 0/-1, blur 0). The SDK's general component shadow is scalar. A correct extra Canvas cast would require an outside-only rounded clip, real Gaussian paint, opposite offsets and a bounded 6-VP outward paint region that survives the panel's ancestors. Current Hero/Editor `.clip(true)` would crop an ordinary outward overlay, so a correct full outer wrapper must be designed before adding that cast. It is not replaced by a hard inner ring or removed for convenience. Exact native ShadowOptions blur equivalence is also not qualified by the px-conversion test.

Other unresolved visual fidelity: high-contrast and reduced-motion handling, Flutter's 160/360-ms style/material transitions, liquid pointer optics/refraction, mode-dependent `p.glassEdge` versus the existing shared `Index.line()` border color, full density/width/theme combinations and exact pixels. These remain explicit parity gaps; ordinary borders are part of the design and must not all be deleted merely because the original complaint mentioned frames.

## Verification evidence

`run-glass-style-alignment.cjs a2`: **23/23 PASS**, 0 failed/skipped/cancelled, 1214.3826 ms; eight source inputs and ten actual Flutter/SDK references match before/after. Qualification `PASS_SCOPED_ACTUAL_GLASS_STYLE_CALLS`. Log SHA-256 `879937fe8ffd41d46d3f08259a0c70c3b23db4f76fb269a442781d1cdf5c2273`. See `glass-style-alignment-a2-result.json`, `-inputs-before.json`, `-inputs-after.json`, and `-tests.log`.

Eight checks execute the actual Index methods and Builder guards/attributes; fifteen execute the actual shared inset methods. UIContext density conversion, ArkUI child/attribute calls, Canvas/Path2D/gradient calls and timers are controlled seams. The Builder harness changes only the outer ArkUI Stack-child syntax into a JS block while retaining the actual guards, child calls, props and attributes; it does not produce a second handwritten layout implementation. The existing 25 component checks plus the updated 15 inset checks also passed together (40/40, 4641.2322 ms) before the preview-mode-only a2 change.

Development invocation history is retained in task output: the first Index-only harness ran 6/7 before failing on a missing `muted` extraction dependency, then the actual `muted`/`accent` methods were included and 7/7 passed. That was a harness dependency error, not a runtime/product rendering qualification. Earlier `glass-style-alignment-final-*` remains preserved as the 22-check pre-preview-mode source freeze; **a2 is the final current-source scope**. A patch context mismatch was rejected before changing source and was corrected against freshly read lines.

This task's SDK build, device rendering and exact pixel match are **NOT_RUN**. Root must verify `RecessedGlassRelief` actually emits in the product graph and qualifies the immutable package containing the a2 source. v31's older helper/module/build proof is not substituted for this new module. Full HMOS/Flutter parity and the all-interface frame complaint remain **OPEN** until current-device evidence supports them.
