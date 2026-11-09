const { test, before, after } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../.tools/qa/node_modules/playwright');

let browser;
before(async () => { browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || 'msedge' }); });
after(async () => { await browser?.close(); });

async function app(t) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 }, reducedMotion: 'reduce' });
  page.setDefaultTimeout(5000);
  t.after(() => page.close());
  await page.route('**/*', async route => {
    const url = new URL(route.request().url());
    if (url.hostname !== 'lazytune.test') return route.abort();
    const file = path.basename(url.pathname) || 'index.html';
    const contentType = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' }[path.extname(file)];
    await route.fulfill({ body: await fs.readFile(path.join(__dirname, '../dist', file)), contentType });
  });
  await page.addInitScript(() => {
    const track = { id: 'dQw4w9WgXcQ', title: 'Track', artist: 'Artist', artist_id: 'UCartist', album: '', artwork: '', duration: 180 };
    const playback = { track, playing: false, position: 100, duration: 180, volume: .5, muted: false, seekable: true, liked: false, ad: false, ready: true, signed_in: false, queue: [track], error: null };
    const listeners = {}, pending = [];
    window.qa = {
      playback, pending,
      emit(patch) { Object.assign(playback, patch); listeners.playback?.({ payload: structuredClone(playback) }); },
      finish(error = false) { const p = pending.shift(); error ? p.reject('Player unavailable') : p.resolve(null); },
    };
    window.__TAURI__ = {
      event: { listen: async (name, callback) => { listeners[name] = callback; } },
      core: { invoke: async (command, args) => {
        if (command === 'bootstrap') return { platform: 'windows', settings: {}, playback, version: 'test', warning: null };
        if (command === 'player_command') return new Promise((resolve, reject) => pending.push({ ...args.command, resolve, reject }));
        if (command === 'catalog') return { shelves: [{ title: 'Artists', items: [{ id: 'UCartist', title: 'Artist', subtitle: '', artwork: 'https://yt3.googleusercontent.com/artist', playable: false }, { id: 'VLplaylist', title: 'Playlist', subtitle: '', artwork: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg', playable: false }, { id: 'MPREalbum', title: 'Album', subtitle: 'Artist', artwork: '', playable: false }, { id: track.id, title: 'Track', subtitle: 'Artist', artwork: '', playable: true }] }], continuation: null, subscription: args.query.type==='browse'&&args.query.value==='UCartist'?{channel_id:'UCofficial',followed:false}:null };
        if (command === 'plugin_status') return [];
        if (command === 'lyrics') return { lines: Array.from({ length: 40 }, (_, i) => ({ time: i * 5, text: `Lyric line ${i}` })), plain: '', source: 'Test' };
        if (command === 'apply_theme') { window.qa.appliedTheme = args.theme; return args.theme; }
        if (command === 'save_settings') { window.qa.savedSettings = args; return null; }
        return null;
      } },
    };
  });
  await page.goto('http://lazytune.test');
  await page.waitForFunction(() => document.querySelector('[aria-label="Seek"]')?.value === '100');
  return page;
}

async function input(page, label, value, commit = false) {
  await page.getByRole('slider', { name: label, exact: true }).evaluate((e, { value, commit }) => {
    e.value = String(value);
    e.dispatchEvent(new Event('input', { bubbles: true }));
    if (commit) e.dispatchEvent(new Event('change', { bubbles: true }));
  }, { value, commit });
}
async function value(page, label) { return Number(await page.getByRole('slider', { name: label, exact: true }).inputValue()); }

test('volume thumb stays with the drag between delayed player reports', async t => {
  const page = await app(t);
  await input(page, 'Volume', .75);
  await page.evaluate(() => qa.finish());
  await page.evaluate(() => qa.emit({ volume: .5 }));
  assert.equal(await value(page, 'Volume'), .75);
  await input(page, 'Volume', .8, true);
  await page.evaluate(() => qa.finish());
  await page.evaluate(() => qa.emit({ volume: .75 }));
  assert.equal(await value(page, 'Volume'), .8);
  await page.evaluate(() => qa.emit({ volume: .8 }));
  await page.evaluate(() => qa.emit({ volume: .4 }));
  assert.equal(await value(page, 'Volume'), .4, 'external changes resume after acknowledgement');
});

test('seek holds its target through delayed reports and older completions', async t => {
  const page = await app(t);
  await input(page, 'Seek', 20, true);
  await page.evaluate(() => qa.finish());
  await page.evaluate(() => qa.emit({ position: 100 }));
  assert.equal(await value(page, 'Seek'), 20);
  await page.evaluate(() => qa.emit({ position: 20 }));
  await page.evaluate(() => qa.emit({ position: 21 }));
  assert.equal(await value(page, 'Seek'), 21);
  await input(page, 'Seek', 0, true);
  await input(page, 'Seek', 60);
  await page.evaluate(() => qa.finish());
  assert.equal(await value(page, 'Seek'), 60, 'an older seek must not finish a newer drag');
});

test('rapid volume input coalesces and a failure restores the reported value', async t => {
  const page = await app(t);
  await input(page, 'Volume', .6);
  await input(page, 'Volume', .7);
  await input(page, 'Volume', .9, true);
  assert.deepEqual(await page.evaluate(() => qa.pending.map(p => p.value)), [.6]);
  await page.evaluate(() => qa.finish());
  assert.deepEqual(await page.evaluate(() => qa.pending.map(p => p.value)), [.9]);
  await page.evaluate(() => qa.finish(true));
  assert.equal(await value(page, 'Volume'), .5);
  assert.match(await page.getByRole('status').innerText(), /Player unavailable/);
});

test('track changes and cancelled drags clear pending seek previews', async t => {
  const page = await app(t);
  await input(page, 'Seek', 40, true);
  await page.evaluate(() => qa.emit({ track: { ...qa.playback.track, id: 'xsdTzjogYBM' }, position: 0 }));
  await page.evaluate(() => qa.finish());
  assert.equal(await value(page, 'Seek'), 0);
  await input(page, 'Seek', 60);
  await page.getByRole('slider', { name: 'Seek', exact: true }).dispatchEvent('pointercancel');
  assert.equal(await value(page, 'Seek'), 0);
});

test('selected surfaces share glass and search has one focus and blur layer', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  const selected = await page.locator('.sidebar .active,.queue-row.current').evaluateAll(nodes => nodes.map(e => {
    const s = getComputedStyle(e); return { background: s.backgroundColor, blur: s.backdropFilter, shadow: s.boxShadow };
  }));
  assert.equal(selected.length, 2);
  assert.deepEqual(selected[0], selected[1]);
  assert.equal(selected[0].blur, 'blur(14px)');
  await page.getByRole('textbox', { name: 'Search music' }).focus();
  const search = await page.locator('.search').evaluate(e => {
    const outer = getComputedStyle(e), inner = getComputedStyle(e.querySelector('input'));
    return { outline: outer.outlineStyle, blur: outer.backdropFilter, innerBlur: inner.backdropFilter, innerOutline: inner.outlineStyle, background: inner.backgroundColor };
  });
  assert.deepEqual(search, { outline: 'solid', blur: 'blur(14px)', innerBlur: 'none', innerOutline: 'none', background: 'rgba(0, 0, 0, 0)' });
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  assert.equal(await page.getByRole('tab', { name: 'Appearance' }).evaluate(e => getComputedStyle(e).backgroundColor), selected[0].background);
  assert.equal(await page.getByRole('tab', { name: 'Appearance' }).getAttribute('aria-selected'), 'true');
  await page.getByRole('button', { name: 'Account', exact: true }).click();
  assert.equal(await page.locator('.account').evaluate(e => e.classList.contains('active')), true);
});

test('artist and playlist backgrounds follow navigation and back history', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Discover', exact: true }).click();
  await page.locator('.card-open').filter({ hasText: /^Artist$/ }).first().click();
  assert.equal(await page.locator('.artwork-background img').getAttribute('src'), 'https://yt3.googleusercontent.com/artist');
  await page.locator('.card-open').filter({ hasText: /^Playlist$/ }).click();
  assert.equal(await page.locator('.artwork-background img').getAttribute('src'), 'https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg');
  await page.getByRole('button', { name: 'Go back', exact: true }).click();
  assert.equal(await page.locator('.artwork-background img').getAttribute('src'), 'https://yt3.googleusercontent.com/artist');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  assert.equal(await page.locator('.artwork-background img').count(), 0);
  await page.getByRole('button', { name: 'Go back', exact: true }).click();
  assert.equal(await page.locator('.artwork-background img').count(), 0);
});

test('video uses one highlighted toggle, including in a narrow window', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  assert.equal(await page.getByRole('button', { name: 'Video', exact: true }).count(), 0);
  const toggle = page.getByRole('button', { name: 'Toggle video', exact: true });
  await toggle.click();
  assert.equal(await toggle.getAttribute('aria-pressed'), 'true');
  assert.equal(await toggle.evaluate(e => getComputedStyle(e).backgroundColor), 'rgba(0, 0, 0, 0)');
  assert.equal(await toggle.locator('svg').evaluate(e => getComputedStyle(e).fill), 'rgb(221, 221, 221)');
  assert.equal(await page.locator('.embedded-page .page-heading').count(), 0);
  assert.equal(await page.locator('#player-surface').count(), 1);
  assert.equal(await page.getByRole('button', { name: 'Now Playing', exact: true }).getAttribute('aria-current'), 'page');
  await page.setViewportSize({ width: 740, height: 680 });
  assert.equal(await toggle.isVisible(), true);
  await toggle.click();
  assert.equal(await toggle.getAttribute('aria-pressed'), 'false');
  assert.equal(await page.getByRole('heading', { name: 'Now Playing', exact: true }).count(), 1);
  assert.equal(await page.locator('#player-surface').count(), 0);
  assert.equal(await page.evaluate(() => qa.pending.length), 0, 'switching views must not restart playback');
});

test('add to queue stays transparent and still queues the track', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Discover', exact: true }).click();
  const add = page.getByRole('button', { name: 'Add to queue', exact: true }).first();
  assert.equal(await add.evaluate(e => getComputedStyle(e).backgroundColor), 'rgba(0, 0, 0, 0)');
  await add.click();
  assert.deepEqual(await page.evaluate(() => qa.pending.map(p => ({ type: p.type, value: p.value }))), [{ type: 'enqueue', value: 'dQw4w9WgXcQ' }]);
  assert.equal(await page.getByRole('heading', { name: 'Discover', exact: true }).count(), 1);
});

test('lyrics match the cover and metadata height as the window and title change', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  for (const width of [1280, 1024]) {
    await page.setViewportSize({ width, height: 820 });
    await page.evaluate(() => qa.emit({ track: { ...qa.playback.track, title: 'A longer song title that wraps onto another line in a narrow player' } }));
    const sizes = await page.evaluate(() => {
      const art = document.querySelector('.now-artwork').getBoundingClientRect();
      const lyrics = document.querySelector('.lyrics').getBoundingClientRect();
      const lines = document.querySelector('.lyric-lines');
      return { art: art.height, lyrics: lyrics.height, topDifference: Math.abs(art.top - lyrics.top), scrollable: lines.scrollHeight > lines.clientHeight };
    });
    assert.ok(Math.abs(sizes.art - sizes.lyrics) < 2, JSON.stringify(sizes));
    assert.ok(sizes.topDifference < 2);
    assert.equal(sizes.scrollable, true);
  }
});

test('mute animates the speaker state and unmutes using the same button', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Mute', exact: true }).click();
  assert.equal(await page.evaluate(() => qa.pending[0].type), 'mute');
  await page.evaluate(() => { qa.emit({ muted: true }); qa.finish(); });
  const mute = page.getByRole('button', { name: 'Unmute', exact: true });
  assert.equal(await mute.getAttribute('aria-pressed'), 'true');
  assert.equal(await mute.locator('.sound-waves').evaluate(e => getComputedStyle(e).opacity), '0');
  assert.equal(await mute.locator('.mute-slash').evaluate(e => getComputedStyle(e).strokeDashoffset), '0px');
  await mute.click();
  await page.evaluate(() => { qa.emit({ muted: false }); qa.finish(); });
  assert.equal(await page.getByRole('button', { name: 'Mute', exact: true }).getAttribute('aria-pressed'), 'false');
});

test('glassy playback bar defaults off and its appearance setting saves automatically', async t => {
  const page = await app(t);
  assert.equal(await page.locator('.transport').evaluate(e => getComputedStyle(e).backdropFilter), 'none');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const glass = page.getByRole('checkbox', { name: /Glassy playback bar/ });
  assert.equal(await glass.isChecked(), false);
  await glass.check();
  await page.waitForFunction(() => document.querySelector('.app').classList.contains('glassy-player'));
  assert.equal(await page.evaluate(() => qa.appliedTheme.glassy_player), true);
  assert.match(await page.locator('.transport').evaluate(e => getComputedStyle(e).backdropFilter), /blur\(22px\)/);
  await glass.uncheck();
  await page.waitForFunction(() => !document.querySelector('.app').classList.contains('glassy-player'));
  assert.equal(await page.evaluate(() => qa.appliedTheme.glassy_player), false);
});

test('playlist and album play overlays start playback without opening the collection', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Discover', exact: true }).click();
  await page.getByRole('button', { name: 'Play playlist', exact: true }).first().click();
  await page.getByRole('button', { name: 'Play album', exact: true }).first().click();
  assert.deepEqual(await page.evaluate(() => qa.pending.map(p => ({ type: p.type, value: p.value }))), [{ type: 'play_collection', value: 'VLplaylist' }, { type: 'play_collection', value: 'MPREalbum' }]);
  assert.equal(await page.getByRole('heading', { name: 'Discover', exact: true }).count(), 1);
});

test('a failed cover becomes a placeholder instead of a broken image', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Discover', exact: true }).click();
  const card = page.locator('.card-open').filter({ hasText: /^Artist$/ }).first();
  await card.locator('.cover.fallback').waitFor();
  assert.equal(await card.locator('img').count(), 0);
});

test('automatic music videos default off and the application setting saves', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('tab', { name: 'Application', exact: true }).click();
  const automatic = page.getByRole('checkbox', { name: /Automatically select music videos/ });
  assert.equal(await automatic.isChecked(), false);
  await automatic.check();
  await page.waitForFunction(() => qa.savedSettings?.autoVideo === true);
  await automatic.uncheck();
  await page.waitForFunction(() => qa.savedSettings?.autoVideo === false);
});

test('dropdowns are readable and only content or inputs allow text selection', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const option = page.locator('select option').first();
  assert.deepEqual(await option.evaluate(e => ({ background: getComputedStyle(e).backgroundColor, color: getComputedStyle(e).color })), { background: 'rgb(34, 34, 34)', color: 'rgb(221, 221, 221)' });
  assert.equal(await page.getByRole('button', { name: 'Discover', exact: true }).evaluate(e => getComputedStyle(e).userSelect), 'none');
  assert.equal(await page.getByRole('textbox', { name: 'Search music' }).evaluate(e => getComputedStyle(e).userSelect), 'text');
  await page.getByRole('button', { name: 'Discover', exact: true }).click();
  const card = page.locator('.card-open').filter({ hasText: /^Artist$/ }).first();
  assert.equal(await card.locator('strong').evaluate(e => getComputedStyle(e).userSelect), 'text');
  await card.evaluate(e => { const range=document.createRange();range.selectNodeContents(e.querySelector('strong'));const selection=getSelection();selection.removeAllRanges();selection.addRange(range);e.dispatchEvent(new MouseEvent('click',{bubbles:true,detail:1})); });
  assert.equal(await page.getByRole('heading', { name: 'Discover', exact: true }).count(), 1, 'selecting a card title should not open it');
  await page.locator('.card-open').filter({ hasText: /^AlbumArtist$/ }).first().click();
  assert.equal(await page.getByRole('heading', { name: 'Album', exact: true }).count(), 1, 'an old selection must not block other cards');
});

test('the current artist opens their page from Now Playing and the bottom bar', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  await page.locator('.now-artwork .artist-link').click();
  assert.equal(await page.getByRole('heading', { name: 'Artist', exact: true }).count(), 1);
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  await page.locator('.current-track .artist-link').click();
  assert.equal(await page.getByRole('heading', { name: 'Artist', exact: true }).count(), 1);
  assert.equal(await page.evaluate(() => qa.pending.length), 0, 'opening an artist should not change playback');
});

test('the seek slider is unavailable while the next song loads', async t => {
  const page = await app(t);
  await page.evaluate(() => qa.emit({ track: { ...qa.playback.track, id: 'xsdTzjogYBM' }, seekable: false, position: 0, duration: 0 }));
  assert.equal(await page.getByRole('slider', { name: 'Seek', exact: true }).isDisabled(), true);
  assert.equal(await value(page, 'Seek'), 0);
  await page.evaluate(() => qa.emit({ seekable: true, position: 0, duration: 200 }));
  assert.equal(await page.getByRole('slider', { name: 'Seek', exact: true }).isEnabled(), true);
  await input(page, 'Seek', 30, true);
  assert.deepEqual(await page.evaluate(() => qa.pending[0].value), { position: 30, track_id: 'xsdTzjogYBM' });
});

test('artists can be followed and a failed unfollow preserves the state', async t => {
  const page = await app(t);
  await page.evaluate(() => qa.emit({ signed_in: true }));
  await page.locator('.current-track .artist-link').click();
  await page.getByRole('button', { name: 'Follow', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => qa.pending[0].value), { channel_id: 'UCofficial', follow: true });
  await page.evaluate(() => qa.finish());
  await page.getByRole('button', { name: 'Following', exact: true }).click();
  await page.evaluate(() => qa.finish(true));
  assert.equal(await page.getByRole('button', { name: 'Following', exact: true }).count(), 1);
});

test('an old artist follow button is hidden while another artist loads', async t => {
  const page = await app(t);
  await page.locator('.current-track .artist-link').click();
  await page.getByRole('button', { name: 'Follow', exact: true }).waitFor();
  await page.evaluate(() => {
    const original=window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke=(command,args)=>command==='catalog'?new Promise(resolve=>{qa.resolveCatalog=resolve;}):original(command,args);
    qa.emit({track:{...qa.playback.track,artist:'Next Artist',artist_id:'UCnext'}});
  });
  await page.locator('.current-track .artist-link').click();
  assert.equal(await page.getByRole('button', { name: 'Follow', exact: true }).count(), 0);
  await page.evaluate(() => qa.resolveCatalog({shelves:[],continuation:null,subscription:{channel_id:'UCnext',followed:false}}));
  await page.getByRole('button', { name: 'Follow', exact: true }).waitFor();
});
