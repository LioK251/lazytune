const views = {
  search: ['search.webp', 'lazytune search results showing music by Vansire, with playback controls along the bottom.'],
  album: ['album.webp', 'An album open in lazytune, with artwork behind its track list.'],
  settings: ['settings.webp', 'lazytune appearance settings with artwork colors and glass effects enabled.'],
};
const tabs = [...document.querySelectorAll('[role="tab"]')];
function selectView(tab) {
  const [file, alt] = views[tab.dataset.view];
  for (const other of tabs) {
    other.setAttribute('aria-selected', String(other === tab));
    other.tabIndex = other === tab ? 0 : -1;
  }
  const image = document.querySelector('#preview-image');
  image.src = `/assets/${file}`;
  image.alt = alt;
  document.querySelector('#app-preview').setAttribute('aria-labelledby', tab.id);
  document.querySelector('#expand-preview').href = image.src;
}
for (const tab of tabs) {
  tab.addEventListener('click', () => selectView(tab));
  tab.addEventListener('keydown', event => {
    const index = tabs.indexOf(tab);
    let next;
    if (event.key === 'ArrowRight') next = (index + 1) % tabs.length;
    else if (event.key === 'ArrowLeft') next = (index + tabs.length - 1) % tabs.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = tabs.length - 1;
    else return;
    event.preventDefault();
    tabs[next].focus();
    selectView(tabs[next]);
  });
}

const repo = 'https://github.com/LioK251/lazytune';
const formats = [
  ['windows', 'windows-x64.exe', 'Download EXE'],
  ['macos', 'macos-arm64.dmg', 'Apple Silicon'],
  ['macos', 'macos-x64.dmg', 'Intel'],
  ['linux', 'linux-x64.AppImage', 'AppImage'],
  ['linux', 'linux-x64.deb', 'DEB'],
  ['linux', 'linux-x64.rpm', 'RPM'],
];
async function loadRelease() {
  const status = document.querySelector('#release-status');
  try {
    const response = await fetch('https://api.github.com/repos/LioK251/lazytune/releases?per_page=5', {
      headers: { Accept: 'application/vnd.github+json' }, signal: AbortSignal.timeout(8000),
    });
    if (!response.ok) throw new Error('Releases unavailable');
    const releases = await response.json();
    if (!Array.isArray(releases)) throw new Error('Invalid release response');
    const release = releases.find(r => !r.draft && Array.isArray(r.assets) && r.assets.some(a => typeof a.name === 'string' && a.name.startsWith('lazytune-')));
    if (!release) return;
    let downloads = 0;
    for (const [platform, suffix, label] of formats) {
      const asset = release.assets.find(a => typeof a.name === 'string' && a.name.startsWith('lazytune-') && a.name.endsWith(`-${suffix}`) && typeof a.browser_download_url === 'string' && a.browser_download_url.startsWith(`${repo}/releases/download/`));
      if (!asset) continue;
      const row = document.querySelector(`[data-platform="${platform}"] .download-links`);
      row.querySelector('.pending-download')?.remove();
      const wrapper = document.createElement('div');
      wrapper.className = 'download-link';
      const link = document.createElement('a');
      link.className = 'button secondary';
      link.href = asset.browser_download_url;
      link.textContent = label;
      wrapper.append(link);
      if (Number.isFinite(asset.size) && asset.size > 0) {
        const size = document.createElement('small');
        size.textContent = `${(asset.size / 1048576).toFixed(1)} MB`;
        wrapper.append(size);
      }
      row.append(wrapper);
      downloads++;
    }
    if (!downloads) return;
    const name = typeof release.tag_name === 'string' ? release.tag_name.slice(0, 40) : 'Latest release';
    status.textContent = `${name}${release.prerelease ? ' preview' : ''}. Free and open source.`;
    if (typeof release.html_url === 'string' && release.html_url.startsWith(`${repo}/releases/tag/`)) document.querySelector('#release-link').href = release.html_url;
  } catch {
    status.textContent = 'Downloads could not be checked. See GitHub releases for the latest files.';
    for (const pending of document.querySelectorAll('.pending-download')) pending.textContent = 'Check GitHub releases';
  }
}
loadRelease();
