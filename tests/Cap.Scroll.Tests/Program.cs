using Cap.Annotation;
using Cap.Core;

int assertions = 0;
void Check(bool condition, string message)
{
    assertions++;
    if (!condition) throw new InvalidOperationException(message);
}

PixelFrame Page(int start, int height = 240, int width = 160)
{
    var pixels = new byte[width * height * 4];
    for (int y = 0; y < height; y++)
        for (int x = 0; x < width; x++)
        {
            uint hash = unchecked((uint)((y + start) * 73856093 ^ x * 19349663));
            hash ^= hash >> 13; hash *= 1274126177; hash ^= hash >> 16;
            int p = (y * width + x) * 4;
            pixels[p] = (byte)hash;
            pixels[p + 1] = (byte)(hash >> 8);
            pixels[p + 2] = (byte)(hash >> 16);
            pixels[p + 3] = 255;
        }
    return new(width, height, pixels);
}

var stitcher = new ScrollingCaptureStitcher(Page(0));
Check(stitcher.TryAppend(Page(100)) == ScrollAppendResult.Appended, "A real 140-row overlap should append.");
Check(stitcher.Height == 340, "Only 100 new rows should be retained.");
Check(stitcher.CreateFrame().Pixels.SequenceEqual(Page(0, 340).Pixels), "Every output row must equal the original page.");
Check(stitcher.TryAppend(Page(100)) == ScrollAppendResult.Unchanged, "Repeated viewport must not duplicate rows.");
Check(stitcher.TryAppend(Page(0)) == ScrollAppendResult.NoReliableOverlap, "Reverse scrolling must be rejected.");
Check(stitcher.Height == 340, "Rejected frames must not change output.");
Check(stitcher.TryAppend(Page(320)) == ScrollAppendResult.NoReliableOverlap, "Less than 20% overlap must be rejected.");
Check(stitcher.TryAppend(Page(200)) == ScrollAppendResult.Appended, "A second forward viewport should append.");
Check(stitcher.Undo(), "Last append must be undoable.");
Check(stitcher.Height == 340, "Undo should remove exactly the last appended rows.");
Check(stitcher.TryAppend(Page(190)) == ScrollAppendResult.Appended, "Alignment must work after undo.");
Check(stitcher.CreateFrame().Pixels.SequenceEqual(Page(0, 430).Pixels), "Undo and reappend must preserve exact page pixels.");

var limited = new ScrollingCaptureStitcher(Page(0), maximumPixels: 160 * 300, maximumHeight: 1000);
Check(limited.TryAppend(Page(100)) == ScrollAppendResult.LimitReached, "Pixel limit must reject before mutation.");
Check(limited.Height == 240 && !limited.CanUndo, "Limit rejection must preserve the first frame.");
var heightLimited = new ScrollingCaptureStitcher(Page(0), maximumHeight: 300);
Check(heightLimited.TryAppend(Page(100)) == ScrollAppendResult.LimitReached, "Height limit must reject before mutation.");

var blank = Page(0);
Array.Fill(blank.Pixels, (byte)255);
var ambiguous = new ScrollingCaptureStitcher(blank);
Check(ambiguous.TryAppend(blank) == ScrollAppendResult.Unchanged, "Blank repeated frames must not append.");
var wrongSize = Page(100, width: 161);
try { stitcher.TryAppend(wrongSize); throw new InvalidOperationException("Mismatched size accepted."); }
catch (ArgumentException) { assertions++; }

var stickyFirst = Page(0);
var stickyNext = Page(100);
Array.Copy(stickyFirst.Pixels, 0, stickyNext.Pixels, 0, stickyFirst.Stride * 20);
var sticky = new ScrollingCaptureStitcher(stickyFirst);
Check(sticky.TryAppend(stickyNext) == ScrollAppendResult.Appended, "Small sticky header should not prevent body alignment.");
Check(sticky.CreateFrame().Pixels.SequenceEqual(Page(0, 340).Pixels), "Sticky header must only appear once.");

var footerFirst = Page(0);
var footerNext = Page(100);
Array.Copy(footerFirst.Pixels, footerFirst.Pixels.Length - footerFirst.Stride * 12,
    footerNext.Pixels, footerNext.Pixels.Length - footerNext.Stride * 12, footerFirst.Stride * 12);
var footer = new ScrollingCaptureStitcher(footerFirst);
Check(footer.TryAppend(footerNext) == ScrollAppendResult.NoReliableOverlap, "Stationary footer must not be copied into seams.");

var paddedFooterFirst = Page(0);
var paddedFooterNext = Page(100);
Array.Copy(paddedFooterFirst.Pixels, paddedFooterFirst.Pixels.Length - paddedFooterFirst.Stride * 24,
    paddedFooterNext.Pixels, paddedFooterNext.Pixels.Length - paddedFooterNext.Stride * 24, paddedFooterFirst.Stride * 24);
Array.Fill(paddedFooterFirst.Pixels, (byte)255, paddedFooterFirst.Pixels.Length - paddedFooterFirst.Stride * 14, paddedFooterFirst.Stride * 14);
Array.Fill(paddedFooterNext.Pixels, (byte)255, paddedFooterNext.Pixels.Length - paddedFooterNext.Stride * 14, paddedFooterNext.Stride * 14);
Check(new ScrollingCaptureStitcher(paddedFooterFirst).TryAppend(paddedFooterNext) == ScrollAppendResult.NoReliableOverlap,
    "Fixed footer with blank bottom padding must be rejected.");

var blankFooterFirst = Page(0);
var blankFooterNext = Page(100);
Array.Fill(blankFooterFirst.Pixels, (byte)255, blankFooterFirst.Pixels.Length - blankFooterFirst.Stride * 10, blankFooterFirst.Stride * 10);
Array.Fill(blankFooterNext.Pixels, (byte)255, blankFooterNext.Pixels.Length - blankFooterNext.Stride * 10, blankFooterNext.Stride * 10);
Check(new ScrollingCaptureStitcher(blankFooterFirst).TryAppend(blankFooterNext) == ScrollAppendResult.NoReliableOverlap,
    "Even an untextured fixed bottom bar must not become an incorrect seam.");

Console.WriteLine($"PASS: {assertions} scrolling capture assertions.");
if (args.Contains("--native-smoke")) NativeCaptureSmoke.Run();
