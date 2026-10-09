# v31 Music inset paint alignment

Observed 2026-10-09. Scope: `pages/MusicPanel.ets` inner current-track and selected-playlist-row decoration. Index remains the owner of the outer music material, palette, depth and radius; no other panel receives this paint layer.

The previous painter only drew nested gradient strokes for neumorphism/clay and returned without painting paper/fluent/brutalist/industrial. The current-track and selected-row backgrounds also applied an ink-alpha fill regardless of style. Flutter uses `NeumorphicSurface(fill: false)` at depth `-0.8` for those two regions, so both child backgrounds now stay transparent and flat paints no inset or frame.

References are actual files in `build/win-cloud-20261005/lib/`: `music/music_panel.dart:274,516`, `neumorphic_controls.dart:15,186,347,460`, and `experimental_controls.dart:171,226-328,347-408`. Exact bytes/hashes are retained in `music-ui-inset-final-inputs-before.json` and `music-ui-inset-final-inputs-after.json`. The SDK Canvas declaration is also captured because this implementation uses its `Path2D`, evenodd fill/clip and Gaussian filter APIs. Canvas/path coordinates and filter lengths use VP.

For a positive material depth `d`, the music relief is `-0.8*d`. The painter keeps `s=min(2,0.8*d)` for wall geometry and `f=min(1,s)` for opacity. Zero/negative/non-finite material depth, flat, unknown style, and unavailable/small Canvas dimensions clear the previous frame without drawing a replacement. The latter guards are local geometry protection, not Flutter pixel-equality evidence.

| Style | Inner decoration for `fill=false` |
| --- | --- |
| flat | Clear only; no fill, stroke or frame. |
| neumorphism | Two blurred complement walls, shade at `(+1.25*s,+1.25*s)` and light at the opposite offset; blur 2 VP; colors mix the actual palette surface toward black/white using Flutter's light/dark factors. |
| paper | 1 VP dark contour and 1 VP light-to-dark gradient edge deflated 1 VP. |
| clay | Opposing complement walls at `1.5*s`, blur 4 VP; separate 1 VP white highlight deflated 1 VP. |
| fluent | 1 VP dark contour; shadow-to-light gradient edge of width 0.8 VP, deflated 1 VP. |
| brutalist | Theme ink contour of width 1.8 VP; ink-to-transparent inner edge of width 1 VP, deflated 1.4 VP. |
| industrial | Dark contour of width `2*s`; shadow-to-light edge of width 1.5 VP, deflated 1.5 VP; 0.7 VP white inner contour deflated 3 VP. |

Neumorphism/clay clip the unshifted rounded shape before painting the difference between an exterior rectangle and each shifted rounded opening with evenodd fill. This preserves the child center instead of painting a filled background or replacing an inner shadow with several hard frames. The filter and clip are enclosed in `save`/`restore`, so the following clay contour is unblurred. Radius is supplied by Index and bounded to the available size; it is not assigned a separate local style scale.

All five watched inset properties (style, depth, dark, surface, radius) redraw. Resizes coalesce to one pending callback; disappearance cancels it and clears the timer identity. Canvas remains decoration-only with `HitTestMode.None`, so playback, selection and drag callbacks retain ownership of interaction.

`MusicPanelContent` now accepts `surface: string`, default `#FFFFFF`, and forwards it to both inset painters. Index must supply its actual `this.surface()` for palette-dependent neumorphic casts. This prop does not fill or alter the outer material.

## Scoped verification

`run-music-ui-inset.cjs final` executed 13 actual-source tests: 13 passed, 0 failed/skipped/cancelled, 1110.4796 ms. Five test/runtime source inputs and four design/SDK references match before and after. Qualification is `PASS_SCOPED_ACTUAL_INSET_PAINT_CALLS`.

Separately, the existing 25 actual component-method checks plus these 13 checks passed together (38/38, 4141.5568 ms). That combined invocation was not a hash-frozen full-product suite; the final frozen report above qualifies only the 13 inset checks.

The harness executes the verbatim production fields/methods and actual `Appearance.mix`. It records SDK Canvas/Path2D/gradient calls and controls timers. Expected color, alpha, deflation, width, shift, blur and paint order assertions are derived from the Flutter source. It does not rasterize Canvas, execute native Gaussian blur, render ArkUI, or compare device pixels.

Frozen production file: `MusicPanel.ets`, 31,324 bytes, SHA-256 `00f88c76a6e3666f4aca62f138eaa331930ced13b6d507b6c902f7ecd20dc44b`. Test log SHA-256 `1784df5a6842c7b85474f5e5f1e4a78a90f273b9cfecef11c0c588c35b68b345`.

SDK product build, device rendering, exact pixel comparison, high-contrast rendering and animated style transitions are `NOT_RUN` in this scoped verification. Root owns SDK/device acceptance. These results do not close the original all-interface unknown-frame complaint or full Flutter parity.
