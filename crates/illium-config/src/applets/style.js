// Injected by Illium into a remote site whose manifest declares a style sheet.
// It only paints the theme colors and appends that sheet; see docs/applets.md.
(() => {
  'use strict';
  const webview = window.chrome && window.chrome.webview;
  if (!webview || window !== window.top) return;
  const css = '__ILLIUM_CSS__';
  const colors = ['bg', 'surface', 'overlay', 'fg', 'muted', 'accent', 'green', 'yellow', 'red'];
  let theme = null;
  // The script runs before <html> exists; the first paint lands on DOMContentLoaded.
  const paint = () => {
    const root = document.documentElement;
    if (!root || !theme) return;
    for (const name of colors) root.style.setProperty(`--illium-${name}`, theme[name]);
    root.style.colorScheme = theme.dark ? 'dark' : 'light';
    if (!document.getElementById('illium-style')) {
      const style = document.createElement('style');
      style.id = 'illium-style';
      style.textContent = css;
      root.appendChild(style);
    }
  };
  document.addEventListener('DOMContentLoaded', paint);
  webview.addEventListener('message', event => {
    const message = event.data;
    if (!message || message.type !== 'theme') return;
    theme = message.value;
    paint();
  });
  webview.postMessage({ type: 'ready' });
})();
