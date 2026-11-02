# Screenshot utility icon

The current application icon is the image supplied by the user on 2026-10-24, exported with the existing WPF asset builder. The supplied master is preserved byte-for-byte; no redesign or production dependency was added. Since v1.2.4, the builder fits the visible artwork to 96% of the icon canvas instead of retaining the master's excessive transparent margins.

## Assets

- `src/Cap.App/Assets/cap-master.png`: the user's unmodified 1254 × 1254 PNG with a transparent outer background.
- `src/Cap.App/Assets/cap.png`: 256 × 256 PNG exported from the master.
- `src/Cap.App/Assets/cap.ico`: PNG-encoded ICO frames at 16, 20, 24, 32, 40, 48, 64, 128 and 256 pixels.

The supplied artwork is a white rounded square with blue capture corners and a blue camera. The builder finds the visible content bounds, ignores isolated alpha specks, then scales uniformly and centers the image with a small safety margin. PNG and all ICO sizes use the same fit for the executable, window and tray. The previous artwork is retained under `.artifacts/ui-backup/20261024-user-camera-icon`; the pre-fit exports are in `.artifacts/ui-backup/20261024-icon-content-fit`.

## Previous icon generation provenance (superseded)

Mode: built-in `image_gen` (not CLI).

Initial output: `C:\Users\Tomoto\.codex\generated_images\01a16e2e-71d4-7492-8e69-436c6b882622\exec-068875d7-9eb7-4ebd-8d15-7803c6d6f5d7.png`

Selected output: `C:\Users\Tomoto\.codex\generated_images\01a16e2e-71d4-7492-8e69-436c6b882622\exec-7dc11cbe-52bf-4e17-b203-c22a7fbef697.png`

Initial prompt:

```text
Use case: logo-brand. Asset type: Windows screenshot utility application icon, a single production-ready square raster icon. Create one centered modern blue rounded-rectangle tile with a crisp white capture selection symbol: exactly FOUR clearly complete L-shaped rounded corners arranged around a wide rectangular empty capture area. At lower-right inside this capture area include one very small simple white selection arrow pointer, unobtrusive and geometrically clean; the four capture corners remain visible and complete. Flat vector-like design with broad readable geometry and strong silhouette, subtle azure-to-cobalt gradient only on tile. Tile fills about 90% of square canvas with modest transparent padding and generously rounded outer corners. White symbol fills about 60% of tile width. Straight front-facing 2D view, no perspective. Intentionally simple enough to read at 16px, balanced professional desktop tool visual. Actual transparent pixels outside the rounded rectangle. No red recording dot, no circles, no camera lens, no letters, no words, no border around canvas, no mockup, no drop shadow, no heavy 3D, no texture, no watermark. Render just the icon artwork.
```

Edge-cleanup edit prompt:

```text
Use case: precise-object-edit. Edit the provided app icon. Keep the exact blue rounded rectangle, white four capture corners and white pointer geometry and composition. Change ONLY the outer boundary: remove every tiny stray cyan or blue speck, fringe, irregular jagged protrusion and noise outside the clean smooth rounded rectangle. Make the outer edge perfectly clean antialiased smooth geometric rounded-rectangle silhouette and set all remaining outside pixels truly transparent. Preserve interior artwork and blue gradient unchanged. No shadow, no other added elements. Production Windows application icon.
```

## Reproduction

Run the asset builder with a compatible .NET 8 SDK:

```powershell
dotnet run --project tools/Cap.AssetBuilder -- src/Cap.App/Assets src/Cap.App/Assets/cap-master.png
```

The builder reads the PNG with `BitmapCacheOption.OnLoad`, preserves transparency, and renders each icon size independently using WPF high-quality bitmap scaling. The master must be square and at least 256 pixels wide. Existing assets and the previous builder were backed up in `.artifacts/ui-backup/icons/20261024-generated` before replacement.
