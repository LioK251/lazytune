# lazytune website

Static HTML, CSS, and JavaScript. No frontend build or production dependencies.

Serve with `python -m http.server 4173 --directory website` from the repository root. Vercel deploys pushes to `main` through the connected GitHub repository. Keep the project's Root Directory set to `website` so only the site is published.

The dark silver design uses the existing app icon and self-hosted Geist (license in `assets/GEIST-LICENSE.txt`). Design variance 5, motion intensity 3, visual density 3: restrained native CSS with feedback on controls and reduced-motion support.

Screenshots render the built Rust/WASM interface in an isolated preview session with public music metadata. They contain no signed-in account details, credentials, or private library data. Feature images show complete app windows at their natural proportions, with smaller WebP variants for narrow screens and links to full-size images. The lyrics example uses the traditional song Scarborough Fair. Keep screenshots in sync with the app when its layout changes.

Downloads come from the public GitHub Releases API. Draft releases stay hidden. Until a release is published, the page says downloads are being prepared. API failures link to GitHub releases instead of inventing download links. OS requirements and signing notices are included beside downloads.

Run `node --test website/website.test.cjs` from the repository root after installing the existing Playwright QA dependency.
