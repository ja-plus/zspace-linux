/* 统一中文/西文字体栈（渲染进程注入，由 background.js 在每个 webContents 的
 * dom-ready 阶段执行）。
 *
 * 现象：页面上中文是黑体、西文（数字、英文单词）却是 Times New Roman 那样的
 * 衬线体，同一个界面里两种字型混排。
 *
 * 根因（2026-11-18 用 CDP 的 CSS.getPlatformFontsForNode 直读实际渲染字体）：
 *   NAS 页面把根节点的字体栈写成了**全角逗号**：
 *     <div id="app" style='font-family: "Microsoft YaHei，Arial，Verdana，sans-serif";'>
 *   Chromium 把整串当成一个字体名，必然找不到，于是
 *     · 西文 → UA 默认字体 Times New Roman（衬线）
 *     · 中文 → 回退 Source Han Sans SC（黑体）
 *   Windows 上 UA 默认字体是 Arial 一类的无衬线，所以官方没发现这个问题。
 *   同一份页面在 Wine 里因为默认回退落在 Noto Sans CJK SC，也看不出来。
 *
 * 做法（只改 font-family，不碰别的样式）：
 *   1. 给 <html> 打上统一栈 → 让"继承来的"文字不再落到 UA 默认的衬线体；
 *   2. 把带全角逗号的 inline font-family 就地改写为同一个栈（!important，
 *      因为它是 inline 声明，普通规则盖不住）；
 *   3. MutationObserver 兜住 Vue 后插入/重写的节点。
 * 图标字体（iconfont）的元素自己有 font-family，且不含全角逗号，不会被命中。
 */
(function () {
  if (window.__zsFontFix) return;
  window.__zsFontFix = 1;

  // 顺序与官方 Windows 客户端一致（页面本来就想用微软雅黑），后面依次是
  // 各发行版常见的黑体，最后才是通用 sans-serif —— 保证任何机器都落到无衬线。
  var CANON = '"Microsoft YaHei","Noto Sans CJK SC","Source Han Sans SC",' +
              '"Source Han Sans CN","PingFang SC",Arial,"Helvetica Neue",' +
              'Helvetica,sans-serif';
  var BROKEN = /，/; // 全角逗号：整串被当成一个字体名的标志
  var hits = 0;

  function fixInline(el) {
    var ff = el.style && el.style.fontFamily;
    if (!ff || !BROKEN.test(ff)) return false;
    el.style.setProperty('font-family', CANON, 'important');
    hits++;
    return true;
  }

  function sweep(root) {
    if (!root || root.nodeType !== 1) return;
    fixInline(root);
    var list;
    try { list = root.querySelectorAll('[style]'); } catch (e) { return; }
    for (var i = 0; i < list.length; i++) fixInline(list[i]);
  }

  function boot() {
    var html = document.documentElement;
    if (html) {
      var cur = html.style.fontFamily || '';
      if (cur.indexOf('Microsoft YaHei') < 0) html.style.setProperty('font-family', CANON, 'important');
      sweep(html);
    }
    // Vue 首屏常在 DOMContentLoaded 之后才挂载，补两轮
    setTimeout(function () { sweep(document.documentElement); }, 800);
    setTimeout(function () { sweep(document.documentElement); }, 2500);
    if (hits) console.log('[font] unified ' + hits + ' inline font-family declaration(s)');
  }

  try {
    new MutationObserver(function (muts) {
      for (var i = 0; i < muts.length; i++) {
        var m = muts[i];
        if (m.type === 'attributes') fixInline(m.target);
        else for (var j = 0; j < m.addedNodes.length; j++) sweep(m.addedNodes[j]);
      }
    }).observe(document.documentElement, {
      childList: true, subtree: true, attributes: true, attributeFilter: ['style']
    });
  } catch (e) { /* 观察失败只影响后插入的节点，首屏仍然修好 */ }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
