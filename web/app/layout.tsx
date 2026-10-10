import type { Metadata, Viewport } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Verbatim — research you can check",
  description:
    "A research agent where every sentence carries a receipt. Every claim is verified by a deterministic checker that did not write it.",
};

export const viewport: Viewport = {
  themeColor: "#fbfaf7",
  width: "device-width",
  initialScale: 1,
};

/**
 * Site footer.
 *
 * The three partner tools were previously only visible in the demo video, which
 * meant a judge reading the site could not see the business model or the
 * checkpoint at all. Links to the primary evidence belong on the page itself.
 */
function Footer() {
  return (
    <footer className="colophon">
      <div className="colophon-row">
        <span className="colophon-label">Built at neatHack 2026</span>
        <nav className="colophon-links">
          <a href="https://youtu.be/Zifq7diJUUg">Demo, 2m42s</a>
          <a href="https://github.com/PhiBao/verbatim">Source</a>
          <a href="https://github.com/PhiBao/verbatim/blob/main/LICENSE">MIT</a>
        </nav>
      </div>
      <div className="colophon-row">
        <span className="colophon-label">The agent</span>
        <nav className="colophon-links">
          <a href="https://github.com/PhiBao/verbatim/blob/main/README.md">
            Verifier: five checks, no model
          </a>
        </nav>
      </div>
      <div className="colophon-row">
        <span className="colophon-label">Economics &middot; cfo.ai</span>
        <nav className="colophon-links">
          <a href="https://cfo.ai/s/mAotSQsQFqUlV-xU">
            83.4% gross margin, break-even at 301 customers
          </a>
        </nav>
      </div>
      <p className="colophon-note">
        Traces are posted to neatlogs; every verification check is a guardrail
        span, so a failed check is red in the dashboard for free. Commits carry
        their agent session through Entire.
      </p>
    </footer>
  );
}

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <head>
        {/* The mark: a quotation glyph set in a solid ink tile. Reads as a
            typographic mark rather than a generic icon at 16px. */}
        <link
          rel="icon"
          href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='3' fill='%231a1814'/%3E%3Ctext x='16' y='23' font-family='Georgia,serif' font-size='24' font-weight='700' text-anchor='middle' fill='%23fbfaf7'%3E%E2%80%9C%3C/text%3E%3C/svg%3E"
        />
      </head>
      <body>
        {children}
        <Footer />
      </body>
    </html>
  );
}
