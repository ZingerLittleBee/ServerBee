export default {
  ignore: {
    files: [
      // These runtime shims are loaded through index.html import maps for external widget modules.
      'public/runtime/*.js',
      // Built-in widgets are compiled by apps/web/vite-plugins/builtin-widgets.ts from this glob.
      'src/builtin-widgets/*.widget.tsx'
    ]
  }
}
