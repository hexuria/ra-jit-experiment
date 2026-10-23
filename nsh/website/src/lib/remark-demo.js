import { demoPanelHtml } from './demo.js';

// A page whose frontmatter names a demo (`demo: counter`) shows it after its
// first paragraph: the reader gets the one-line introduction, then the thing
// running. `state` is unused here; it is in the options only so the content
// cache is invalidated when the demos or the player change (see demo.js).
export function remarkDemo({ base = '/' } = {}) {
  return async (tree, file) => {
    const name = file?.data?.astro?.frontmatter?.demo;
    if (!name) return;
    const path = file?.path ?? file?.history?.[0] ?? '';
    const lang = /[\\/]zh-CN[\\/]/.test(path) ? 'zh-CN' : 'en';
    const html = await demoPanelHtml(name, { base, lang });

    const at = tree.children.findIndex((node) => node.type === 'paragraph');
    tree.children.splice(at === -1 ? 0 : at + 1, 0, { type: 'html', value: html });
  };
}
