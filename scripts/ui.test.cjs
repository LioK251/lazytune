const { test, before, after } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../.tools/qa/node_modules/playwright');

let browser;
before(async () => { browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || 'msedge' }); });
after(async () => { await browser?.close(); });

async function app(t, options = {}) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 }, reducedMotion: options.reducedMotion || 'reduce' });
  page.setDefaultTimeout(5000);
  t.after(() => page.close());
  await page.route('**/*', async route => {
    const url = new URL(route.request().url());
    if (url.hostname !== 'lazytune.test') return route.abort();
    const file = path.basename(url.pathname) || 'index.html';
    const contentType = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' }[path.extname(file)];
    await route.fulfill({ body: await fs.readFile(path.join(__dirname, '../dist', file)), contentType });
  });
  await page.addInitScript(options => {
    const track = { id: 'dQw4w9WgXcQ', title: 'Track', artist: 'Artist', artist_id: 'UCartist', album: '', artwork: '', duration: 180 };
    const playback = { track, playing: false, play_requested: false, position: 100, duration: 180, volume: .5, muted: false, seekable: true, liked: false, ad: false, ready: true, signed_in: false, queue: [track], error: null };
    const listeners = {}, pending = [];
    window.qa = {
      playback, pending, commands: [],
      emit(patch) { Object.assign(playback, patch); listeners.playback?.({ payload: structuredClone(playback) }); },
      spectrum(bands) { listeners.visualizer?.({ payload: bands }); },
      finish(error = false) { const p = pending.shift(); error ? p.reject('Player unavailable') : p.resolve(null); },
    };
    window.__TAURI__ = {
      event: { listen: async (name, callback) => { listeners[name] = callback; } },
      core: { invoke: async (command, args) => {
        window.qa.commands.push({ command, args });
        if (command === 'bootstrap') return { platform: options.platform || 'windows', settings: options.settings || {}, playback, version: 'test', warning: options.warning || null };
        if (command === 'check_update') return options.update || null;
        if (command === 'player_command') return new Promise((resolve, reject) => pending.push({ ...args.command, resolve, reject }));
        if (command === 'catalog') return { shelves: [{ title: 'Artists', items: [{ id: 'UCartist', title: 'Artist', subtitle: '', artwork: 'https://yt3.googleusercontent.com/artist', playable: false }, { id: 'VLplaylist', title: 'Playlist', subtitle: '', artwork: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg', playable: false }, { id: 'MPREalbum', title: 'Album', subtitle: 'Artist', artwork: '', playable: false }, { id: track.id, title: 'Track', subtitle: 'Artist', artwork: '', playable: true }] }], continuation: null, subscription: args.query.type==='browse'&&args.query.value==='UCartist'?{channel_id:'UCofficial',followed:false}:null };
        if (command === 'plugin_status') return [];
        if (command === 'lyrics') return { lines: Array.from({ length: 40 }, (_, i) => ({ time: i * 5, text: `Lyric line ${i}` })), plain: '', source: 'Test' };
        if (command === 'apply_theme') { window.qa.appliedTheme = args.theme; return args.theme; }
        if (command === 'save_settings') { window.qa.savedSettings = args; return null; }
        return null;
      } },
    };
  }, options);
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

test('the seek slider accepts a pointer drag while playback reports keep arriving', async t => {
  const page = await app(t);
  const slider = page.getByRole('slider', { name: 'Seek', exact: true });
  const box = await slider.boundingBox();
  await page.mouse.move(box.x + box.width * .55, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * .25, box.y + box.height / 2, { steps: 4 });
  const dragged = await value(page, 'Seek');
  await page.evaluate(() => qa.emit({ position: 101, seekable: true }));
  assert.equal(await value(page, 'Seek'), dragged);
  await page.mouse.up();
  const command = await page.evaluate(() => qa.pending.find(p => p.type === 'seek'));
  assert.ok(command.value.position > 35 && command.value.position < 55);
  assert.equal(command.value.track_id, 'dQw4w9WgXcQ');
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

test('long queue titles cannot push the artwork over the lyrics', async t => {
  const page = await app(t);
  await page.evaluate(() => qa.emit({ queue: [{ ...qa.playback.track, title: 'it was the middle of October and we were young again' }] }));
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  for (const width of [1280, 1129, 1024]) {
    await page.setViewportSize({ width, height: 820 });
    const bounds = await page.evaluate(() => {
      const artwork = document.querySelector('.now-artwork').getBoundingClientRect();
      const lyrics = document.querySelector('.lyrics').getBoundingClientRect();
      return { artworkRight: artwork.right, lyricsLeft: lyrics.left };
    });
    assert.ok(bounds.artworkRight + 20 <= bounds.lyricsLeft, JSON.stringify({ width, ...bounds }));
  }
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

test('glassy playback bar defaults on and its appearance setting saves automatically', async t => {
  const page = await app(t);
  assert.match(await page.locator('.transport').evaluate(e => getComputedStyle(e).backdropFilter), /blur\(22px\)/);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const glass = page.getByRole('checkbox', { name: /Glassy playback bar/ });
  assert.equal(await glass.isChecked(), true);
  assert.equal(await page.getByRole('checkbox', { name: /^Colors from artwork/ }).isChecked(), true);
  assert.equal(await page.getByRole('checkbox', { name: /^Use album artwork as background/ }).isChecked(), true);
  await glass.uncheck();
  await page.waitForFunction(() => !document.querySelector('.app').classList.contains('glassy-player'));
  assert.equal(await page.evaluate(() => qa.appliedTheme.glassy_player), false);
  assert.equal(await page.locator('.transport').evaluate(e => getComputedStyle(e).backdropFilter), 'none');
  await glass.check();
  await page.waitForFunction(() => document.querySelector('.app').classList.contains('glassy-player'));
  assert.equal(await page.evaluate(() => qa.appliedTheme.glassy_player), true);
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

test('album tracks without covers use the album artwork, while artist tracks do not', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Discover', exact: true }).click();
  const albumArt = 'https://yt3.googleusercontent.com/album-test';
  await page.route(albumArt, route => route.fulfill({ contentType: 'image/png', path: path.join(__dirname, '../desktop/icons/icon.png') }));
  await page.evaluate(albumArt => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = async (command, args) => {
      if (command === 'catalog' && args.query.type === 'browse') return { artwork: albumArt, shelves: [{ title: 'Songs', items: [{ id: 'dQw4w9WgXcQ', title: 'Album track', subtitle: 'Artist', artwork: '', playable: true }] }], continuation: null, subscription: null };
      return invoke(command, args);
    };
  }, albumArt);
  await page.locator('.card-open').filter({ hasText: /^AlbumArtist$/ }).first().click();
  const albumTrack = page.locator('.card-open').filter({ hasText: /^Album trackArtist$/ });
  assert.equal(await albumTrack.locator('img').getAttribute('src'), albumArt);
  await page.getByRole('button', { name: 'Go back', exact: true }).click();
  await page.locator('.card-open').filter({ hasText: /^Artist$/ }).first().click();
  await page.locator('.card-open').filter({ hasText: /^Album trackArtist$/ }).locator('.cover.fallback').waitFor();
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

test('short windows keep bounded artwork and full song metadata above playback controls', async t => {
  const page = await app(t);
  await page.evaluate(() => qa.emit({ track: { ...qa.playback.track, title: 'A longer song title that wraps onto another line in a narrow player', artist: 'An artist with a longer name' } }));
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  for (const viewport of [{ width: 1088, height: 742 }, { width: 1024, height: 680 }, { width: 1600, height: 900 }]) {
    await page.setViewportSize(viewport);
    const bounds = await page.evaluate(() => {
      const cover = document.querySelector('.now-artwork>.cover').getBoundingClientRect();
      const metadata = document.querySelector('.now-artwork>p').getBoundingClientRect();
      const footer = document.querySelector('.transport').getBoundingClientRect();
      return { coverWidth: cover.width, coverHeight: cover.height, metadataBottom: metadata.bottom, footerTop: footer.top };
    });
    assert.ok(bounds.coverWidth <= 360 && Math.abs(bounds.coverWidth - bounds.coverHeight) < 1, JSON.stringify({ viewport, bounds }));
    assert.ok(bounds.metadataBottom < bounds.footerTop, JSON.stringify({ viewport, bounds }));
  }
});

test('pending playback offers Pause before the media starts', async t => {
  const page = await app(t);
  await page.evaluate(() => qa.emit({ playing: false, play_requested: true }));
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  assert.equal(await page.evaluate(() => qa.pending[0].type), 'toggle');
  await page.evaluate(() => { qa.finish(); qa.emit({ play_requested: false }); });
  await page.getByRole('button', { name: 'Play', exact: true }).waitFor();
});

test('catalog requests show errors without loading copy', async t => {
  const page = await app(t);
  await page.evaluate(() => {
    const original = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (command, args) => command === 'catalog' ? new Promise((resolve, reject) => { qa.rejectCatalog = reject; }) : original(command, args);
  });
  await page.getByRole('button', { name: 'Discover', exact: true }).click();
  assert.equal(await page.getByText(/^Loading/).count(), 0);
  await page.evaluate(() => qa.rejectCatalog('Search unavailable'));
  await page.getByRole('alert').filter({ hasText: 'Search unavailable' }).waitFor();
});

test('visualizer defaults off, autosaves modes and follows real bands without covering controls', async t => {
  const page = await app(t, { reducedMotion: 'no-preference' });
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const mode = page.getByRole('combobox', { name: 'Audio visualizer', exact: true });
  assert.equal(await mode.inputValue(), 'off');
  await mode.selectOption('spectrum_bars');
  await page.waitForFunction(() => qa.appliedTheme?.visualizer === 'spectrum_bars');
  await page.evaluate(() => qa.emit({ playing: true }));
  await page.waitForFunction(() => qa.commands.some(c => c.command === 'visualizer_active' && c.args.active));
  await page.evaluate(() => qa.spectrum(Array.from({ length: 32 }, (_, i) => i * 8)));
  const bars = page.locator('.visualizer-spectrum>span');
  assert.equal(await bars.count(), 32);
  assert.notEqual(await bars.first().getAttribute('style'), await bars.last().getAttribute('style'));
  assert.deepEqual(await page.locator('.visualizer').evaluate(e => ({ pointerEvents: getComputedStyle(e).pointerEvents, zIndex: getComputedStyle(e).zIndex })), { pointerEvents: 'none', zIndex: '-1' });
  await mode.selectOption('background_pulse');
  await page.locator('.visualizer-pulse').waitFor({ state: 'attached' });
  await page.evaluate(() => qa.spectrum(Array(32).fill(255)));
  assert.match(await page.locator('.visualizer-pulse').getAttribute('style'), /opacity:0\.220/);
  await page.evaluate(() => qa.emit({ playing: false }));
  await page.waitForFunction(() => !document.querySelector('.visualizer').classList.contains('active'));
  assert.match(await page.locator('.visualizer-pulse').getAttribute('style'), /opacity:0\.000/);
  assert.equal(await page.evaluate(() => qa.commands.filter(c => c.command === 'visualizer_active').at(-1).args.active), false);
});

test('hidden windows and reduced motion stop visualizer sampling', async t => {
  const page = await app(t, { reducedMotion: 'no-preference' });
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('combobox', { name: 'Audio visualizer', exact: true }).selectOption('spectrum_bars');
  await page.waitForFunction(() => qa.appliedTheme?.visualizer === 'spectrum_bars');
  await page.evaluate(() => qa.emit({ playing: true }));
  await page.waitForFunction(() => document.querySelector('.visualizer').classList.contains('active'));
  await page.evaluate(() => { Object.defineProperty(document, 'hidden', { configurable: true, value: true }); document.dispatchEvent(new Event('visibilitychange')); });
  await page.waitForFunction(() => !document.querySelector('.visualizer').classList.contains('active'));
  await page.evaluate(() => { delete document.hidden; document.dispatchEvent(new Event('visibilitychange')); });
  await page.waitForFunction(() => document.querySelector('.visualizer').classList.contains('active'));
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.waitForFunction(() => !document.querySelector('.visualizer').classList.contains('active'));
});

test('WebKit platforms disable visualization while preserving imported theme modes', async t => {
  const theme = JSON.parse(await fs.readFile(path.join(__dirname, '../themes/monochrome.lazytune-theme'), 'utf8'));
  for (const platform of ['macos', 'linux']) {
    for (const visualizer of ['background_pulse', 'spectrum_bars']) {
      const page = await app(t, { platform, reducedMotion: 'no-preference', settings: { theme: { ...theme, visualizer } } });
      await page.getByRole('button', { name: 'Settings', exact: true }).click();
      const selector = page.getByRole('combobox', { name: 'Audio visualizer', exact: true });
      assert.equal(await selector.isDisabled(), true);
      assert.equal(await selector.inputValue(), visualizer, 'the imported preference remains available for theme export');
      await page.getByText('Audio capture is unavailable in this webview.', { exact: true }).waitFor();
      await page.evaluate(() => { qa.emit({ playing: true, play_requested: true }); qa.spectrum(Array(32).fill(255)); });
      assert.equal(await page.locator('.visualizer').count(), 0, 'unsupported engines do not render the effect');
      assert.equal(await page.evaluate(() => qa.commands.some(c => c.command === 'visualizer_active' && c.args.active)), false, 'unsupported engines never request sampling');
      assert.equal(await page.evaluate(() => qa.appliedTheme), undefined, 'runtime gating must not rewrite the saved theme');
    }
  }
});

test('sign-in Retry recreates only the login surface and Back returns to account', async t => {
  const page = await app(t);
  await page.getByRole('button', { name: 'Account', exact: true }).click();
  await page.getByRole('button', { name: 'Sign in with Google', exact: true }).click();
  await page.waitForFunction(() => qa.commands.some(c => c.command === 'show_player' && c.args.mode === 'login'));
  await page.getByRole('button', { name: 'Retry', exact: true }).click();
  const retry = await page.evaluate(() => qa.commands.find(c => c.command === 'show_player' && c.args.retry));
  assert.equal(retry.args.mode, 'login');
  assert.ok(retry.args.bounds.width > 0 && retry.args.bounds.height > 0);
  await page.getByRole('button', { name: 'Back', exact: true }).click();
  await page.getByRole('button', { name: 'Sign in with Google', exact: true }).waitFor();
  await page.waitForFunction(() => qa.commands.filter(c => c.command === 'show_player').at(-1).args.mode === 'audio');
});

test('an update is acknowledged after rendering and opens the validated release command', async t => {
  const page = await app(t, { update: { version: '0.1.2', release_url: 'https://github.com/LioK251/lazytune/releases/tag/v0.1.2' } });
  await page.getByText('lazytune 0.1.2 is available', { exact: true }).waitFor();
  await page.waitForFunction(() => qa.commands.some(c => c.command === 'acknowledge_update'));
  assert.equal(await page.evaluate(() => qa.commands.filter(c => c.command === 'acknowledge_update').length), 1);
  await page.getByRole('button', { name: 'View release', exact: true }).click();
  assert.deepEqual(await page.evaluate(() => qa.commands.find(c => c.command === 'open_update').args), { version: '0.1.2' });
  await page.locator('.update-notice').waitFor({ state: 'detached' });
});

test('errors take precedence over update notifications and delay acknowledgement until visible', async t => {
  const page = await app(t, { warning: 'Player unavailable', update: { version: '0.1.2', release_url: 'https://github.com/LioK251/lazytune/releases/tag/v0.1.2' } });
  await page.getByRole('status').filter({ hasText: 'Player unavailable' }).waitFor();
  assert.equal(await page.locator('.update-notice').count(), 0);
  assert.equal(await page.evaluate(() => qa.commands.filter(c => c.command === 'acknowledge_update').length), 0);
  await page.getByRole('button', { name: 'Dismiss notification', exact: true }).click();
  await page.locator('.update-notice').waitFor();
  await page.waitForFunction(() => qa.commands.some(c => c.command === 'acknowledge_update'));
});

test('playback errors appear on every route once and repeated reports do not prolong the toast', async t => {
  const page = await app(t);
  await page.clock.install();
  await page.evaluate(() => qa.emit({ error: 'Playback unavailable' }));
  await page.getByRole('status').filter({ hasText: 'Playback unavailable' }).waitFor();
  await page.clock.fastForward(4000);
  await page.evaluate(() => qa.emit({ error: 'Playback unavailable', position: 101 }));
  await page.clock.fastForward(1100);
  assert.equal(await page.getByRole('status').count(), 0);
  await page.getByRole('button', { name: 'Now Playing', exact: true }).click();
  await page.getByRole('alert').filter({ hasText: 'Playback unavailable' }).waitFor();
});
