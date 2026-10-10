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

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <head>
        {/* The mark: a quotation glyph whose dot is replaced by nothing, set in a
            monospace tile. Reads as a solid tile with a notch at 16px. */}
        <link
          rel="icon"
          href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='3' fill='%231a1814'/%3E%3Ctext x='16' y='23' font-family='Georgia,serif' font-size='24' font-weight='700' text-anchor='middle' fill='%23fbfaf7'%3E%E2%80%9C%3C/text%3E%3C/svg%3E"
        />
      </head>
      <body>{children}</body>
    </html>
  );
}
