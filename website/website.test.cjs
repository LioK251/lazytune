const { test, before, after } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../.tools/qa/node_modules/playwright');
let browser;
before(async () => { browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || 'msedge' }); });
after(async () => { await browser?.close(); });

async function pageFor(t, releases = [], width = 1440, apiStatus = 200) {
  const page = await browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce' });
  t.after(() => page.close());
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.setDefaultTimeout(5000);
  await page.route('**/*', async route => {
    const url = new URL(route.request().url());
    if (url.hostname === 'api.github.com') return route.fulfill({ status: apiStatus, json: releases, headers: { 'Access-Control-Allow-Origin': '*' } });
    assert.equal(url.hostname, 'lazytune.test', `Unexpected external request: ${url.hostname}`);
    const relative = decodeURIComponent(url.pathname).replace(/^\//, '') || 'index.html';
    const file = path.resolve(__dirname, relative);
    assert.ok(file.startsWith(__dirname + path.sep));
    const contentType = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.webp': 'image/webp', '.woff2': 'font/woff2' }[path.extname(file)];
    const headers = { 'Content-Security-Policy': JSON.parse(await fs.readFile(path.join(__dirname, 'vercel.json'))).headers[0].headers.find(h => h.key === 'Content-Security-Policy').value };
    await route.fulfill({ body: await fs.readFile(file), contentType, headers });
  });
  await page.goto('http://lazytune.test');
  await page.evaluate(() => document.fonts.ready);
  t.after(() => assert.deepEqual(errors, []));
  return page;
}

test('screenshots are accessible by keyboard and every image loads', async t => {
  const page = await pageFor(t);
  const search = page.getByRole('tab', { name: 'Search', exact: true });
  await search.focus();
  await page.keyboard.press('ArrowRight');
  assert.equal(await page.getByRole('tab', { name: 'Albums', exact: true }).getAttribute('aria-selected'), 'true');
  assert.match(await page.locator('#preview-image').getAttribute('src'), /album.webp$/);
  await page.keyboard.press('End');
  assert.equal(await page.getByRole('tab', { name: 'Appearance', exact: true }).getAttribute('aria-selected'), 'true');
  assert.match(await page.locator('#expand-preview').getAttribute('href'), /settings.webp$/);
  await page.locator('img').evaluateAll(images => images.forEach(image => { image.loading = 'eager'; }));
  assert.deepEqual(await page.evaluate(async () => {
    await Promise.all([...document.images].map(image => image.decode().catch(() => {})));
    return [...document.images].filter(image => !image.complete || image.naturalWidth === 0).map(image => image.src);
  }), []);
});

test('mobile widths keep navigation, controls, and requirements within the viewport', async t => {
  for (const width of [320, 390, 768]) {
    const page = await pageFor(t, [], width);
    await page.getByText('Installation & system requirements', { exact: true }).click();
    assert.ok(await page.locator('details').getAttribute('open') !== null);
    const layout = await page.evaluate(() => ({ width: innerWidth, scroll: document.documentElement.scrollWidth, motion: getComputedStyle(document.documentElement).scrollBehavior }));
    assert.ok(layout.scroll <= width, JSON.stringify(layout));
    assert.equal(layout.motion, 'auto');
  }
});

test('unpublished releases never produce download links', async t => {
  const page = await pageFor(t, [{ draft: true, tag_name: 'v0.1.0', assets: [] }]);
  assert.match(await page.locator('#release-status').innerText(), /being prepared/);
  assert.equal(await page.locator('.download-links a').count(), 0);
});

test('published assets map to the right platforms and untrusted URLs are rejected', async t => {
  const repo = 'https://github.com/LioK251/lazytune';
  const suffixes = ['windows-x64.exe', 'macos-arm64.dmg', 'macos-x64.dmg', 'linux-x64.AppImage', 'linux-x64.deb', 'linux-x64.rpm'];
  const assets = suffixes.map(suffix => ({ name: `lazytune-0.1.0-${suffix}`, size: 12000000, browser_download_url: `${repo}/releases/download/v0.1.0/lazytune-0.1.0-${suffix}` }));
  assets.unshift({ name: 'lazytune-0.1.0-windows-x64.exe', browser_download_url: 'javascript:alert(1)' });
  const page = await pageFor(t, [
    { tag_name: 'v0.2.0', prerelease: true, assets, html_url: `${repo}/releases/tag/v0.2.0` },
    { tag_name: 'v0.1.0', prerelease: false, assets, html_url: `${repo}/releases/tag/v0.1.0` }
  ]);
  await page.getByRole('link', { name: 'Download EXE', exact: true }).waitFor();
  assert.equal(await page.locator('.download-links a').count(), 6);
  assert.equal(await page.locator('[data-platform="macos"] .download-links a').count(), 2);
  assert.equal(await page.locator('[data-platform="linux"] .download-links a').count(), 3);
  assert.match(await page.locator('#release-status').innerText(), /v0.1.0\. Free/);
  assert.ok((await page.locator('.download-links a').evaluateAll(links => links.map(link => link.href))).every(url => url.startsWith(`${repo}/releases/download/`)));
});

test('GitHub API failures leave a working releases link', async t => {
  const page = await pageFor(t, {}, 1440, 403);
  await page.getByText(/Downloads could not be checked/).waitFor();
  assert.equal(await page.locator('#release-link').getAttribute('href'), 'https://github.com/LioK251/lazytune/releases');
  assert.equal(await page.locator('.download-links a').count(), 0);
});
