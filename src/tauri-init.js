/**
 * 統一的 Tauri 初始化輪詢（overlay.html、panel.html 共用）
 * settings.html 使用 ES module，不需要此檔案。
 *
 * 用法：
 *   <script src="tauri-init.js"></script>
 *   <script>
 *     waitForTauri(function(ctx) {
 *       // ctx.invoke, ctx.win, ctx.listen 可用
 *     });
 *   </script>
 */
function waitForTauri(callback, maxAttempts) {
  var count = 0;
  maxAttempts = maxAttempts || 50;
  function check() {
    if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
      callback({
        invoke: window.__TAURI__.core.invoke,
        win: window.__TAURI__.window.getCurrentWindow(),
        listen: window.__TAURI__.event.listen
      });
    } else if (count++ < maxAttempts) {
      setTimeout(check, 100);
    }
  }
  check();
}
