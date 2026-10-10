#!/usr/bin/env python3
"""Screenshot a shared cfo.ai page, with its tables fully loaded.

Two things get in the way of a clean capture, both hit on the first attempt:

1. A "Keep your access" sign-in modal covers the lower right and a Cloudflare
   challenge runs behind it.
2. Table cells render "Loading..." and only fill in later, so a screenshot taken
   as soon as text appears captures empty tables.

Usage:  python3 scripts/shoot-cfo.py <share-url> <outfile>
"""
import sys
from pathlib import Path

from playwright.sync_api import sync_playwright

url = sys.argv[1]
out = Path(sys.argv[2])
out.parent.mkdir(parents=True, exist_ok=True)

with sync_playwright() as p:
    browser = p.chromium.launch(
        executable_path="/home/kiter/.local/bin/chromium",
        args=["--no-sandbox", "--disable-dev-shm-usage"],
    )
    page = browser.new_page(viewport={"width": 1600, "height": 1400})
    page.goto(url, wait_until="domcontentloaded", timeout=90_000)

    # The shared view occasionally renders a "We couldn't load this scenario"
    # error on first paint. It is transient — the HTML has the real title and a
    # reload brings the content up.
    for attempt in range(4):
        try:
            page.wait_for_function(
                "() => document.body.innerText.length > 400", timeout=25_000
            )
        except Exception:
            pass
        body = page.inner_text("body")
        if "couldn't load" in body or "Try again" in body:
            print(f"attempt {attempt + 1}: transient load error, reloading")
            page.wait_for_timeout(4_000)
            page.reload(wait_until="domcontentloaded", timeout=90_000)
            page.wait_for_timeout(5_000)
            continue
        break
    page.wait_for_timeout(6_000)

    # Dismiss the sign-in modal so it does not sit over the tables.
    for label in ("Dismiss", "Close", "×", "✕", "No thanks", "Continue without", "Got it"):
        try:
            page.get_by_role("button", name=label).first.click(timeout=2_000)
            print(f"dismissed modal via {label!r}")
            break
        except Exception:
            continue
    else:
        # Fall back to pressing Escape, which most dialogs honour.
        page.keyboard.press("Escape")
        print("pressed Escape to dismiss any modal")

    page.wait_for_timeout(2_000)

    # The sign-in modal is a dialog overlay with no stable label. Removing any
    # fixed-position element that covers the lower-right is more reliable than
    # hunting for its close button.
    removed = page.evaluate(
        """() => {
            let n = 0;
            for (const el of document.querySelectorAll('body *')) {
                const cs = getComputedStyle(el);
                if ((cs.position === 'fixed' || cs.position === 'absolute') &&
                    el.offsetWidth > 260 && el.offsetHeight > 220) {
                    const t = (el.innerText || '');
                    if (t.includes('Keep your access') || t.includes('Sign in to hold')) {
                        el.remove(); n++;
                    }
                }
            }
            return n;
        }"""
    )
    print(f"removed {removed} overlay element(s)")

    # Scroll the whole page so lazy-loaded blocks fire, then wait for the
    # "Loading..." placeholders to clear.
    height = page.evaluate("() => document.body.scrollHeight")
    for y in range(0, height, 700):
        page.evaluate(f"() => window.scrollTo(0, {y})")
        page.wait_for_timeout(700)
    page.evaluate("() => window.scrollTo(0, 0)")

    for _ in range(20):
        if "Loading" not in page.inner_text("body"):
            break
        page.wait_for_timeout(3_000)

    text = page.inner_text("body")
    print(f"rendered {len(text)} characters")
    print("---- text ----")
    Path(str(out).replace(".png", ".txt")).write_text(text)
    print(text[:1200])

    page.screenshot(path=str(out), full_page=True)
    print(f"\nwrote {out} ({out.stat().st_size} bytes)")
    browser.close()
