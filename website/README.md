# lazytune website

Static HTML, CSS, and JavaScript. No frontend build or production dependencies.

Serve with `python -m http.server 4173 --directory website` from the repository root. Deploy only this directory with `vercel --cwd website --prod`; never deploy the workspace root.

The dark silver design uses the existing app icon and self-hosted Geist (license in `assets/GEIST-LICENSE.txt`). Design variance 5, motion intensity 3, visual density 3: restrained native CSS with feedback on controls and reduced-motion support.

Screenshots render the built Rust/WASM interface in an isolated preview session with public music metadata. They contain no signed-in account details, credentials, or private library data. Keep them in sync with the app when its layout changes.

Downloads come from the public GitHub Releases API. Draft releases stay hidden. Until a release is published, the page says the preview is being prepared. API failures link to GitHub releases instead of inventing download links. OS requirements and unsigned-preview notices are included beside downloads.

Run `node --test website/website.test.cjs` from the repository root after installing the existing Playwright QA dependency.
