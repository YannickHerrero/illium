// Injected by Illium before any script of a web applet page. See docs/applets.md.
(() => {
  'use strict';
  const webview = window.chrome && window.chrome.webview;
  if (!webview || window.illium || window !== window.top) return;
  const sticky = new Set(['data', 'theme', 'busy', 'error']);
  const colors = ['bg', 'surface', 'overlay', 'fg', 'muted', 'accent', 'green', 'yellow', 'red'];
  const last = new Map();
  const listeners = new Map();
  let opened = false;
  const call = (callback, value) => {
    try {
      callback(value);
    } catch (error) {
      console.error(error);
    }
  };
  // The script runs before <html> exists; the first theme lands on DOMContentLoaded.
  const paint = () => {
    const root = document.documentElement;
    const theme = last.get('theme');
    if (!root || !theme) return;
    for (const name of colors) root.style.setProperty(`--illium-${name}`, theme[name]);
    root.style.colorScheme = theme.dark ? 'dark' : 'light';
  };
  document.addEventListener('DOMContentLoaded', paint);
  webview.addEventListener('message', event => {
    const message = event.data;
    if (!message || typeof message.type !== 'string') return;
    const { type, value } = message;
    if (sticky.has(type)) last.set(type, value);
    if (type === 'theme') paint();
    if (type === 'open') opened = true;
    if (type === 'dismissed') opened = false;
    for (const callback of [...(listeners.get(type) || [])]) call(callback, value);
  });
  window.illium = Object.freeze({
    on(type, callback) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(callback);
      if (sticky.has(type) && last.has(type)) call(callback, last.get(type));
      return () => listeners.get(type).delete(callback);
    },
    action(args) {
      webview.postMessage({ type: 'action', args: args === undefined ? null : args });
    },
    close() {
      webview.postMessage({ type: 'close' });
    },
    get data() {
      return last.has('data') ? last.get('data') : null;
    },
    get theme() {
      return last.has('theme') ? last.get('theme') : null;
    },
    get opened() {
      return opened;
    },
  });
  webview.postMessage({ type: 'ready' });
})();
