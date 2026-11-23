// The user guide as a PDF, for printing or keeping: Chromium prints
// index.html with the app's own fonts (no network needed), the light look,
// no contents rail, each section on a new page and every answer open.
//
//   node docs/guide-pdf.cjs out.pdf 0.8.0
//
// Uses `playwright` or `playwright-core`. ISOBAR_PDF_CHANNEL=chrome prints
// with the installed Google Chrome instead of Playwright's own Chromium.

const path = require("path");
let pw;
try {
  pw = require("playwright");
} catch {
  pw = require("playwright-core");
}

const [out = "isobar-user-guide.pdf", version = ""] = process.argv.slice(2);
const guide = path.join(__dirname, "guide", "index.html");
const fonts = path.join(__dirname, "..", "assets", "fonts");

const faces = [
  ["Barlow", "Barlow-Regular", 400, "normal"],
  ["Barlow", "Barlow-Regular", 400, "italic"],
  ["Barlow", "Barlow-Medium", 500, "normal"],
  ["Barlow", "Barlow-SemiBold", 600, "normal"],
  ["Barlow Condensed", "BarlowCondensed-Medium", 500, "normal"],
  ["Barlow Condensed", "BarlowCondensed-SemiBold", 600, "normal"],
  ["Barlow Condensed", "BarlowCondensed-SemiBold", 700, "normal"],
]
  .map(
    ([family, file, weight, style]) =>
      `@font-face { font-family: '${family}'; src: url('file://${path.join(fonts, file + ".ttf")}'); font-weight: ${weight}; font-style: ${style}; }`
  )
  .join("\n");

const print = `
@media print {
  html, body { background: #fff !important; }
  body::before { display: none !important; }
  .top, nav.toc, .theme, .search, .menu-btn, .switch, .nohits { display: none !important; }
  .wrap { display: block !important; max-width: none !important; padding: 0 !important; margin: 0 !important; }
  body { font-size: 13px; }
  section.s { padding-top: 0; }
  section.s:not(#welcome) { break-before: page; }
  h2, h3, h4 { break-after: avoid; }
  figure, .pair, .tbl, table, tr, .calc, .idea, .tip, .warn, details, .annot { break-inside: avoid; }
  figure.shot { cursor: auto; }
  .calc input[type=range] { display: none; }
  .prose p, .prose li { max-width: none; }
  a { color: inherit; text-decoration: none; }
}`;

(async () => {
  const browser = await pw.chromium.launch(
    process.env.ISOBAR_PDF_CHANNEL ? { channel: process.env.ISOBAR_PDF_CHANNEL } : {}
  );
  const page = await browser.newPage();
  await page.emulateMedia({ colorScheme: "light" });
  // The fonts come from the repository, not Google Fonts.
  await page.route(/fonts\.(googleapis|gstatic)\.com/, (r) => r.abort());
  await page.goto("file://" + guide, { waitUntil: "load" });
  await page.addStyleTag({ content: faces + print });
  await page.evaluate(async () => {
    document.documentElement.dataset.theme = "light";
    document.querySelectorAll("details").forEach((d) => (d.open = true));
    const imgs = [...document.images];
    imgs.forEach((i) => (i.loading = "eager"));
    await Promise.all(
      imgs.map((i) => (i.complete ? null : new Promise((ok) => (i.onload = i.onerror = ok))))
    );
    await document.fonts.ready;
  });
  const label = version ? `Isobar ${version} user guide` : "Isobar user guide";
  await page.pdf({
    path: out,
    format: "A4",
    printBackground: true,
    displayHeaderFooter: true,
    headerTemplate: "<span></span>",
    footerTemplate: `<div style="font:9px sans-serif;color:#667;width:100%;padding:0 13mm;display:flex;justify-content:space-between"><span>${label}</span><span><span class="pageNumber"></span> / <span class="totalPages"></span></span></div>`,
    margin: { top: "14mm", bottom: "16mm", left: "13mm", right: "13mm" },
  });
  await browser.close();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
